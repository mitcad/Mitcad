// SPDX-License-Identifier: MIT
// The INSPECT group of the SOLID tab (U4): Measure, Interference, Section
// Analysis, Print Analysis and Physical Properties, on the model's
// analysis queries (commands.md, "Analysis queries", "Print analysis").
// They show answers; only Section Analysis and Print Analysis leave
// something behind: OK keeps them in the document as analyses (the
// browser's Analysis folder, mitcad#41, mitcad#620), and while one is shown
// the bodies stay cut at its plane, or its overhangs and thin walls stay
// coloured over them.
#include "Commands.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QLabel>
#include <QScrollArea>
#include <QSettings>
#include <QStringList>
#include <QVBoxLayout>
#include <QtLogging>
#include <QtMath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>

#include "../files/Print3d.hpp"
#include "../framework/Appearances.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/CommandSession.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelShapes.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/Theme.hpp"
#include "CommandSupport.hpp"
#include "PrintAnalysis.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {
namespace {

using namespace cmd;

QString n(double value, int decimals = 3) {
  QString text = QString::number(value, 'f', decimals);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

QString xyz(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return QStringLiteral("(%1, %2, %3)").arg(n(a.at(0).toDouble()), n(a.at(1).toDouble()), n(a.at(2).toDouble()));
}

// A circle or arc (mitcad#194): its radius with its centre, and the
// diameter, twice the radius before either is rounded for display.
void describeCircle(double radius, const QString& centre, QStringList& lines, QStringList& log) {
  const double diameter = 2.0 * radius;
  lines << QObject::tr("Radius: %1 mm, centre %2").arg(n(radius), centre);
  lines << QObject::tr("Diameter: %1 mm").arg(n(diameter));
  log << QStringLiteral("radius %1").arg(n(radius)) << QStringLiteral("diameter %1").arg(n(diameter));
}

// What the measure query says of one selection.
QString describeMeasured(const QJsonObject& item, QStringList& log) {
  QStringList lines;
  const QString kind = item.value(QStringLiteral("kind")).toString();
  lines << QStringLiteral("<b>%1</b>").arg(kind);
  const auto add = [&](const char* key, const QString& label, const QString& unit) {
    if (item.contains(QLatin1String(key))) {
      const QString value = n(item.value(QLatin1String(key)).toDouble());
      lines << QStringLiteral("%1: %2 %3").arg(label, value, unit);
      log << QStringLiteral("%1 %2").arg(QString::fromLatin1(key), value);
    }
  };
  add("volume", QObject::tr("Volume"), QStringLiteral("mm³"));
  add("area", QObject::tr("Area"), QStringLiteral("mm²"));
  add("length", QObject::tr("Length"), QStringLiteral("mm"));
  if (item.contains(QStringLiteral("point"))) {
    lines << QObject::tr("Position: %1").arg(xyz(item.value(QStringLiteral("point"))));
    log << QStringLiteral("point %1").arg(xyz(item.value(QStringLiteral("point"))));
  }
  const QJsonObject circle = item.value(QStringLiteral("circle")).toObject();
  if (!circle.isEmpty()) {
    describeCircle(circle.value(QStringLiteral("radius")).toDouble(), xyz(circle.value(QStringLiteral("center"))),
                   lines, log);
  }
  // A cylinder's, cone's or circle's axis, or a construction axis
  // (mitcad#647): its point (the centre) and direction.
  const QJsonObject axis = item.value(QStringLiteral("axis")).toObject();
  if (!axis.isEmpty()) {
    const QString origin = xyz(axis.value(QStringLiteral("origin")));
    const QString direction = xyz(axis.value(QStringLiteral("direction")));
    lines << QObject::tr("Axis: through %1, direction %2").arg(origin, direction);
    log << QStringLiteral("axis %1 along %2").arg(origin, direction);
  }
  return lines.join(QStringLiteral("<br>"));
}

// The panel's rows (mitcad#1010): a name and a value with its unit.
Inspection::Value valueRow(const QString& name, const QString& value) {
  Inspection::Value row;
  row.name = name;
  row.value = value;
  return row;
}

// What the measure query says of one selection as the rows of its group in
// the panel (mitcad#1010): Diameter, Radius, Area, Length, Volume,
// Position, the axis through its point and its direction, as they apply;
// a circle without an axis its centre.
std::vector<Inspection::Value> measuredValues(const QJsonObject& item) {
  std::vector<Inspection::Value> values;
  const QJsonObject circle = item.value(QStringLiteral("circle")).toObject();
  const bool round = circle.value(QStringLiteral("radius")).isDouble();
  if (round) {
    const double radius = circle.value(QStringLiteral("radius")).toDouble();
    values.push_back(valueRow(QObject::tr("Diameter"), QStringLiteral("%1 mm").arg(n(2.0 * radius))));
    values.push_back(valueRow(QObject::tr("Radius"), QStringLiteral("%1 mm").arg(n(radius))));
  }
  const auto add = [&](const char* key, const QString& name, const QString& unit) {
    if (item.value(QLatin1String(key)).isDouble()) {
      values.push_back(valueRow(name, QStringLiteral("%1 %2").arg(n(item.value(QLatin1String(key)).toDouble()), unit)));
    }
  };
  add("area", QObject::tr("Area"), QStringLiteral("mm²"));
  add("length", QObject::tr("Length"), QStringLiteral("mm"));
  add("volume", QObject::tr("Volume"), QStringLiteral("mm³"));
  if (item.contains(QStringLiteral("point"))) {
    values.push_back(valueRow(QObject::tr("Position"), xyz(item.value(QStringLiteral("point")))));
  }
  const QJsonObject axis = item.value(QStringLiteral("axis")).toObject();
  if (!axis.isEmpty()) {
    values.push_back(valueRow(QObject::tr("Axis through"), xyz(axis.value(QStringLiteral("origin")))));
    values.push_back(valueRow(QObject::tr("Axis direction"), xyz(axis.value(QStringLiteral("direction")))));
  } else if (round) {
    values.push_back(valueRow(QObject::tr("Centre"), xyz(circle.value(QStringLiteral("center")))));
  }
  return values;
}

// A pick's kind in the words of its group's title (mitcad#1010): a face's
// or an edge's geometry ("cylindrical face", "straight edge", "sketch
// arc"), else the kind of the pick ("vertex", "body", "plane").
QString pickKind(const SelectionItem& item, const CommandContext& context) {
  const QString kind = SelectionItem::kindName(item.kind);
  if (item.kind != SelectKind::Face && item.kind != SelectKind::Edge && item.kind != SelectKind::SketchCurve) {
    return kind;
  }
  TopoDS_Shape shape;
  try {
    shape = context.itemShape(item);
  } catch (const std::exception&) {
    return kind;
  }
  if (shape.IsNull()) {
    return kind;
  }
  if (item.kind == SelectKind::Face) {
    TopExp_Explorer faces(shape, TopAbs_FACE);
    if (!faces.More()) {
      return kind;
    }
    switch (BRepAdaptor_Surface(TopoDS::Face(faces.Current())).GetType()) {
    case GeomAbs_Plane:
      return QObject::tr("planar face");
    case GeomAbs_Cylinder:
      return QObject::tr("cylindrical face");
    case GeomAbs_Cone:
      return QObject::tr("conical face");
    case GeomAbs_Sphere:
      return QObject::tr("spherical face");
    case GeomAbs_Torus:
      return QObject::tr("toroidal face");
    default:
      return kind;
    }
  }
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (!edges.More()) {
    return kind;
  }
  const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
  const bool sketch = item.kind == SelectKind::SketchCurve;
  switch (curve.GetType()) {
  case GeomAbs_Line:
    return sketch ? QObject::tr("sketch line") : QObject::tr("straight edge");
  case GeomAbs_Circle:
    if (sketch) {
      return curve.IsClosed() ? QObject::tr("sketch circle") : QObject::tr("sketch arc");
    }
    return QObject::tr("circular edge");
  case GeomAbs_Ellipse:
    return sketch ? QObject::tr("sketch ellipse") : QObject::tr("elliptical edge");
  default:
    return kind;
  }
}

// A pick's group in the panel's rows: "Selection 1: cylindrical face"
// (with the Bodies filter "Body 1: body", as its input is named).
Inspection::Group pickGroup(int number, bool bodies, const SelectionItem& item, const CommandContext& context,
                            std::vector<Inspection::Value> values) {
  Inspection::Group group;
  group.title = (bodies ? QObject::tr("Body %1: %2") : QObject::tr("Selection %1: %2"))
                    .arg(number)
                    .arg(pickKind(item, context));
  group.values = std::move(values);
  return group;
}

// The main value of a measurement for its history row (mitcad#1010): of
// two picks the main pair value (the distance), of one its volume,
// diameter, length, area or position, the first it has.
QString mainValue(const Inspection& result) {
  for (const Inspection::Value& value : result.values) {
    if (value.main) {
      return value.value;
    }
  }
  if (result.groups.empty()) {
    return QString();
  }
  const std::vector<Inspection::Value>& values = result.groups.front().values;
  for (const QString& name : {QObject::tr("Volume"), QObject::tr("Diameter"), QObject::tr("Length"),
                              QObject::tr("Area"), QObject::tr("Position")}) {
    for (const Inspection::Value& value : values) {
      if (value.name == name) {
        return value.value;
      }
    }
  }
  return values.empty() ? QString() : values.front().value;
}

// The picks' kinds for a history row: "Face ↔ Edge".
QString pickedKinds(const SelectionItem& first, const SelectionItem* second) {
  const auto kind = [](const SelectionItem& item) {
    QString text = SelectionItem::kindName(item.kind);
    if (!text.isEmpty()) {
      text[0] = text[0].toUpper();
    }
    return text;
  };
  QString text = kind(first);
  if (second != nullptr) {
    text += QStringLiteral(" ↔ ") + kind(*second);
  }
  return text;
}

// The axis of a measured item drawn as a line through its point while the
// panel is open (mitcad#647): a round face's line runs its length along
// the axis (a whole cylinder's, from its area) and a radius beyond each
// end, a circle's twice its radius each way, a cone's (no radius) the
// square root of its area each way, a construction axis 25 mm each way.
TopoDS_Shape axisLine(const QJsonObject& item) {
  const QJsonObject axis = item.value(QStringLiteral("axis")).toObject();
  if (axis.isEmpty()) {
    return TopoDS_Shape();
  }
  const gp_Pnt origin = pointOf(axis.value(QStringLiteral("origin")));
  const gp_Vec direction = vectorOf(axis.value(QStringLiteral("direction")));
  if (direction.Magnitude() < 1e-9) {
    return TopoDS_Shape();
  }
  const double radius = item.value(QStringLiteral("circle")).toObject().value(QStringLiteral("radius")).toDouble();
  const double area = item.value(QStringLiteral("area")).toDouble();
  double half = 25.0;
  if (radius > 0.0 && area > 0.0) {
    const double circumference = 4.0 * std::asin(1.0) * radius;
    half = area / circumference / 2.0 + radius;
  } else if (radius > 0.0) {
    half = 2.0 * radius;
  } else if (area > 0.0) {
    half = std::sqrt(area);
  }
  const gp_Vec along = direction.Normalized() * half;
  return BRepBuilderAPI_MakeEdge(origin.Translated(-along), origin.Translated(along)).Edge();
}

// A pick in the words of Measure's history (mitcad#648): "Face of Body1",
// a body's name, "Sketch curve of Sketch1", a construction feature's name,
// or an origin datum ("Plane XY").
QString pickedName(const SelectionItem& item, const CommandContext& context) {
  QString kind = SelectionItem::kindName(item.kind);
  if (!kind.isEmpty()) {
    kind[0] = kind[0].toUpper();
  }
  try {
    switch (item.kind) {
    case SelectKind::Body:
      return context.bodyName(item.owner);
    case SelectKind::Face:
    case SelectKind::Edge:
    case SelectKind::Vertex:
      return QObject::tr("%1 of %2").arg(kind, context.bodyName(item.owner));
    case SelectKind::SketchCurve:
    case SelectKind::SketchPoint:
      return QObject::tr("%1 of %2").arg(kind, context.featureName(item.owner));
    case SelectKind::Plane:
    case SelectKind::Axis:
    case SelectKind::Point: {
      if (item.owner == QStringLiteral("origin")) {
        return QObject::tr("Origin");
      }
      const QString name = context.featureName(item.owner);
      return name != item.owner ? name : QStringLiteral("%1 %2").arg(kind, item.owner.toUpper());
    }
    default:
      break;
    }
  } catch (const std::exception&) {
    // The kind alone: the model could not name it.
  }
  return kind;
}

// The picks of a measurement for the history: "Face of Body1 ↔ Edge of
// Body2".
QString pickedNames(const SelectionItem& first, const SelectionItem* second, const CommandContext& context) {
  QString text = pickedName(first, context);
  if (second != nullptr) {
    text += QStringLiteral(" ↔ ") + pickedName(*second, context);
  }
  return text;
}

// The main values of a measure query's answer for the history
// (mitcad#648): a single pick's volume, area, length or position; a circle's
// radius and diameter; between two picks the distance, the angle, the
// offset and the axis and centre distances.
QStringList recordValues(const QJsonObject& answer) {
  QStringList values;
  const bool pair = answer.contains(QStringLiteral("b"));
  const auto millimetres = [](const QString& label, double value) {
    return QStringLiteral("%1 %2 mm").arg(label, n(value));
  };
  for (const QString& key : {QStringLiteral("a"), QStringLiteral("b")}) {
    const QJsonObject item = answer.value(key).toObject();
    if (item.isEmpty()) {
      continue;
    }
    if (!pair) {
      if (item.value(QStringLiteral("volume")).isDouble()) {
        values << QStringLiteral("%1 %2 mm³")
                      .arg(QObject::tr("Volume"), n(item.value(QStringLiteral("volume")).toDouble()));
      }
      if (item.value(QStringLiteral("area")).isDouble()) {
        values << QStringLiteral("%1 %2 mm²")
                      .arg(QObject::tr("Area"), n(item.value(QStringLiteral("area")).toDouble()));
      }
      if (item.value(QStringLiteral("length")).isDouble()) {
        values << millimetres(QObject::tr("Length"), item.value(QStringLiteral("length")).toDouble());
      }
      if (item.contains(QStringLiteral("point"))) {
        values << QStringLiteral("%1 %2").arg(QObject::tr("Position"), xyz(item.value(QStringLiteral("point"))));
      }
    }
    const QJsonObject circle = item.value(QStringLiteral("circle")).toObject();
    if (circle.value(QStringLiteral("radius")).isDouble()) {
      const double radius = circle.value(QStringLiteral("radius")).toDouble();
      values << millimetres(QObject::tr("Radius"), radius) << millimetres(QObject::tr("Diameter"), 2.0 * radius);
    }
  }
  const QJsonObject between = answer.value(QStringLiteral("between")).toObject();
  if (between.value(QStringLiteral("distance")).isDouble()) {
    values << millimetres(QObject::tr("Distance"), between.value(QStringLiteral("distance")).toDouble());
  }
  if (between.value(QStringLiteral("angle")).isDouble()) {
    values << QStringLiteral("%1 %2°")
                  .arg(QObject::tr("Angle"), degrees(between.value(QStringLiteral("angle")).toDouble()));
  }
  const std::pair<const char*, QString> distances[] = {
      {"offset", QObject::tr("Offset")},
      {"axis_distance", QObject::tr("Axis distance")},
      {"center_distance", QObject::tr("Centre distance")}};
  for (const auto& [key, label] : distances) {
    if (between.value(QLatin1String(key)).isDouble()) {
      values << millimetres(label, between.value(QLatin1String(key)).toDouble());
    }
  }
  return values;
}

// A pick as the measure query takes it, with the occurrence that places it
// (mitcad#646), so that the model measures it where the view shows it.
QJsonValue measureRef(const SelectionItem& item) {
  QJsonObject ref;
  if (item.kind == SelectKind::Plane || item.kind == SelectKind::Axis || item.kind == SelectKind::Point) {
    ref = QJsonObject{{QStringLiteral("datum"), item.owner}};
  } else {
    const QJsonValue reference = item.reference();
    if (!reference.isObject()) {
      return reference;
    }
    ref = reference.toObject();
  }
  if (!item.occurrence.isEmpty()) {
    ref.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  return ref;
}

bool isSketchItem(const SelectionItem& item) {
  return item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint;
}

QString pointText(const gp_Pnt& p) { return QStringLiteral("(%1, %2, %3)").arg(n(p.X()), n(p.Y()), n(p.Z())); }

// The single straight edge of a shape, if it is one.
std::optional<gp_Dir> lineDirection(const TopoDS_Shape& shape) {
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (!edges.More()) {
    return std::nullopt;
  }
  const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
  edges.Next();
  if (edges.More() || curve.GetType() != GeomAbs_Line) {
    return std::nullopt;
  }
  return curve.Line().Direction();
}

// Measure with sketch geometry (P9): the model measures bodies and datums;
// sketch curves and points (in sketch mode, or shown sketches) are measured
// here on the shapes the view shows.
QString describeShape(const SelectionItem& item, const TopoDS_Shape& shape, QStringList& log) {
  QStringList lines;
  lines << QStringLiteral("<b>%1</b>").arg(SelectionItem::kindName(item.kind));
  if (shape.ShapeType() == TopAbs_VERTEX) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(shape));
    lines << QObject::tr("Position: %1").arg(pointText(p));
    log << QStringLiteral("point %1").arg(pointText(p));
    return lines.join(QStringLiteral("<br>"));
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props);
  lines << QObject::tr("Length: %1 mm").arg(n(props.Mass()));
  log << QStringLiteral("length %1").arg(n(props.Mass()));
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (edges.More()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Circ circle = curve.Circle();
      describeCircle(circle.Radius(), pointText(circle.Location()), lines, log);
    }
  }
  return lines.join(QStringLiteral("<br>"));
}

// A sketch curve's or point's rows in the panel (mitcad#1010), in the
// order of a model pick's: Diameter, Radius, Length, Position, a circle's
// centre.
std::vector<Inspection::Value> shapeValues(const TopoDS_Shape& shape) {
  std::vector<Inspection::Value> values;
  if (shape.ShapeType() == TopAbs_VERTEX) {
    values.push_back(valueRow(QObject::tr("Position"), pointText(BRep_Tool::Pnt(TopoDS::Vertex(shape)))));
    return values;
  }
  QString centre;
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (edges.More()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Circ circle = curve.Circle();
      values.push_back(valueRow(QObject::tr("Diameter"), QStringLiteral("%1 mm").arg(n(2.0 * circle.Radius()))));
      values.push_back(valueRow(QObject::tr("Radius"), QStringLiteral("%1 mm").arg(n(circle.Radius()))));
      centre = pointText(circle.Location());
    }
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props);
  values.push_back(valueRow(QObject::tr("Length"), QStringLiteral("%1 mm").arg(n(props.Mass()))));
  if (!centre.isEmpty()) {
    values.push_back(valueRow(QObject::tr("Centre"), centre));
  }
  return values;
}

Inspection measureShapes(const SelectionItem& first, const SelectionItem* second, const CommandContext& context) {
  Inspection result;
  QStringList log;
  QStringList parts;
  const TopoDS_Shape a = context.itemShape(first);
  const TopoDS_Shape b = second != nullptr ? context.itemShape(*second) : TopoDS_Shape();
  if (a.IsNull() || (second != nullptr && b.IsNull())) {
    result.error = QObject::tr("The selection is not there any more.");
    return result;
  }
  parts << describeShape(first, a, log);
  // The panel's rows (mitcad#1010): the pair's values, then each pick's.
  result.groups.push_back(pickGroup(1, false, first, context, shapeValues(a)));
  if (second != nullptr) {
    parts << describeShape(*second, b, log);
    result.groups.push_back(pickGroup(2, false, *second, context, shapeValues(b)));
    QStringList lines;
    BRepExtrema_DistShapeShape distance(a, b);
    if (distance.IsDone() && distance.NbSolution() > 0) {
      lines << QObject::tr("<b>Distance: %1 mm</b>").arg(n(distance.Value()));
      log << QStringLiteral("distance %1").arg(n(distance.Value()));
      const gp_Pnt p = distance.PointOnShape1(1);
      const gp_Pnt q = distance.PointOnShape2(1);
      if (p.Distance(q) > 1e-9) {
        result.shape = BRepBuilderAPI_MakeEdge(p, q).Edge();
      }
      lines << QObject::tr("From %1 to %2").arg(pointText(p), pointText(q));
      Inspection::Value row = valueRow(QObject::tr("Distance"), QStringLiteral("%1 mm").arg(n(distance.Value())));
      row.main = true;
      row.tip = QObject::tr("From %1 to %2").arg(pointText(p), pointText(q));
      result.values.push_back(row);
    }
    const auto da = lineDirection(a);
    const auto db = lineDirection(b);
    if (da && db) {
      // The angle between the lines, 0 to 90 degrees.
      const QString angle = degrees(std::acos(std::min(1.0, std::abs(da->Dot(*db)))));
      lines << QObject::tr("<b>Angle: %1°</b>").arg(angle);
      log << QStringLiteral("angle %1 deg").arg(angle);
      result.values.push_back(valueRow(QObject::tr("Angle"), QStringLiteral("%1°").arg(angle)));
    }
    parts << lines.join(QStringLiteral("<br>"));
  }
  result.text = parts.join(QStringLiteral("<br><br>"));
  result.log = log.join(QStringLiteral(", "));
  // For the history (mitcad#648): the logged values in words; of two picks
  // their circles, the distance and the angle.
  result.record.picked = pickedNames(first, second, context);
  for (const QString& entry : log) {
    const QString key = entry.section(QLatin1Char(' '), 0, 0);
    const QString value = entry.section(QLatin1Char(' '), 1);
    if (key == QStringLiteral("angle")) {
      result.record.values << QStringLiteral("%1 %2°").arg(QObject::tr("Angle"), value.section(QLatin1Char(' '), 0, 0));
    } else if (key == QStringLiteral("point") && second == nullptr) {
      result.record.values << QStringLiteral("%1 %2").arg(QObject::tr("Position"), value);
    } else if (key == QStringLiteral("radius") || key == QStringLiteral("diameter") ||
               key == QStringLiteral("distance") || (key == QStringLiteral("length") && second == nullptr)) {
      QString label = key;
      label[0] = label[0].toUpper();
      result.record.values << QStringLiteral("%1 %2 mm").arg(label, value);
    }
  }
  result.record.main = mainValue(result);
  result.record.kinds = pickedKinds(first, second);
  return result;
}

// ---------------------------------------------------------------------------
// Measure

CommandDef measureCommand() {
  CommandDef def;
  def.id = QStringLiteral("inspect.measure");
  def.name = QObject::tr("Measure");
  def.icon = QStringLiteral("measure");
  def.tooltip = QObject::tr("Distances, angles, lengths, areas and positions of what is picked");
  def.shortcut = QKeySequence(Qt::Key_I);
  def.group = QStringLiteral("INSPECT");
  def.pinned = true;
  def.keywords = {QStringLiteral("distance"), QStringLiteral("angle"), QStringLiteral("dimension")};
  // Faces, edges and vertices, which Measure picks by default, or
  // whole bodies (the Selection Filter; a body would win over its faces
  // in the view, so it is a choice); datums that are shown (construction
  // features, the origin when its light bulb is on); and sketch curves and
  // points (P9), also in sketch mode, where Measure starts without leaving
  // the sketch. With Faces, Edges and Vertices a body's face, edge or
  // vertex under the pointer comes before a shown sketch's curve or point
  // there (a sketch in the body's middle plane, mitcad#869), except the
  // curves and points of the sketch being edited; Sketch Curves and Points
  // picks only those.
  def.mode = CommandDef::Mode::Any;
  const SelectFilter kinds = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex |
                             SelectKind::Plane | SelectKind::Axis | SelectKind::Point |
                             SelectKind::SketchCurve | SelectKind::SketchPoint;
  const auto bodies = [](const CommandState& s) {
    return s.choice(QStringLiteral("filter")) == QStringLiteral("bodies");
  };
  const auto sketches = [](const CommandState& s) {
    return s.choice(QStringLiteral("filter")) == QStringLiteral("sketch");
  };
  const auto geometry = [bodies, sketches](const CommandState& s) { return !bodies(s) && !sketches(s); };
  const SelectFilter sketchKinds = SelectKind::SketchCurve | SelectKind::SketchPoint;
  def.inputs = {
      choiceInput(QStringLiteral("filter"), QObject::tr("Selection Filter"),
                  {{QStringLiteral("geometry"), QObject::tr("Faces, Edges and Vertices")},
                   {QStringLiteral("bodies"), QObject::tr("Bodies")},
                   {QStringLiteral("sketch"), QObject::tr("Sketch Curves and Points")}},
                  QStringLiteral("geometry")),
      selectionInput(QStringLiteral("a"), QObject::tr("Selection 1"), kinds, 1, 1)
          .withVisible(geometry)
          .withSketchesBehind(),
      selectionInput(QStringLiteral("b"), QObject::tr("Selection 2"), kinds, 0, 1)
          .withVisible(geometry)
          .withSketchesBehind(),
      selectionInput(QStringLiteral("body_a"), QObject::tr("Body 1"), SelectKind::Body, 1, 1).withVisible(bodies),
      selectionInput(QStringLiteral("body_b"), QObject::tr("Body 2"), SelectKind::Body, 0, 1).withVisible(bodies),
      selectionInput(QStringLiteral("sketch_a"), QObject::tr("Selection 1"), sketchKinds, 1, 1).withVisible(sketches),
      selectionInput(QStringLiteral("sketch_b"), QObject::tr("Selection 2"), sketchKinds, 0, 1).withVisible(sketches),
  };
  for (InputDef& input : def.inputs) {
    input.showsOrigin = false;
  }
  // The filter's choices on a line of their own under its label, whole at
  // the panel's default width (mitcad#1010).
  def.inputs.front().fieldBelow = true;
  // It only shows: Close (mitcad#1010). What Keep as Dimension and the
  // Annotate menu add are undo steps of their own outside the panel.
  def.closeOnly = true;
  def.inspect = [bodies, sketches](const CommandState& state, const CommandContext& context) {
    Inspection result;
    // The inputs that hold the picks, also for the history (mitcad#648).
    const QStringList inputs = bodies(state)     ? QStringList{QStringLiteral("body_a"), QStringLiteral("body_b")}
                               : sketches(state) ? QStringList{QStringLiteral("sketch_a"), QStringLiteral("sketch_b")}
                                                 : QStringList{QStringLiteral("a"), QStringLiteral("b")};
    const Selection& first = state.items(inputs.at(0));
    const Selection& second = state.items(inputs.at(1));
    if (isSketchItem(first.first()) || (!second.isEmpty() && isSketchItem(second.first()))) {
      result = measureShapes(first.first(), second.isEmpty() ? nullptr : &second.first(), context);
      result.record.inputs = inputs;
      return result;
    }
    QJsonObject request{{QStringLiteral("query"), QStringLiteral("measure")},
                        {QStringLiteral("a"), measureRef(first.first())}};
    if (!second.isEmpty()) {
      request.insert(QStringLiteral("b"), measureRef(second.first()));
    }
    const QJsonObject answer = context.queryObject(request);
    QStringList log;
    QStringList parts;
    parts << describeMeasured(answer.value(QStringLiteral("a")).toObject(), log);
    if (answer.contains(QStringLiteral("b"))) {
      parts << describeMeasured(answer.value(QStringLiteral("b")).toObject(), log);
    }
    // Drawn over the view: the axes of the picks (mitcad#647), the closest
    // distance and the axis distance.
    TopoDS_Compound drawn;
    BRep_Builder builder;
    builder.MakeCompound(drawn);
    int axes = 0;
    for (const QString& key : {QStringLiteral("a"), QStringLiteral("b")}) {
      const TopoDS_Shape line = axisLine(answer.value(key).toObject());
      if (!line.IsNull()) {
        builder.Add(drawn, line);
        ++axes;
      }
    }
    const QJsonObject between = answer.value(QStringLiteral("between")).toObject();
    if (!between.isEmpty()) {
      QStringList lines;
      const double distance = between.value(QStringLiteral("distance")).toDouble();
      lines << QObject::tr("<b>Distance: %1 mm</b>").arg(n(distance));
      log << QStringLiteral("distance %1").arg(n(distance));
      // The panel's rows (mitcad#1010): the distance first, larger, the
      // points it is measured between and Δx, Δy and Δz in its tooltip.
      Inspection::Value distanceRow = valueRow(QObject::tr("Distance"), QStringLiteral("%1 mm").arg(n(distance)));
      distanceRow.main = true;
      result.values.push_back(distanceRow);
      if (between.value(QStringLiteral("angle")).isDouble()) {
        const QString angle = degrees(between.value(QStringLiteral("angle")).toDouble());
        lines << QObject::tr("<b>Angle: %1°</b>").arg(angle);
        log << QStringLiteral("angle %1 deg").arg(angle);
        result.values.push_back(valueRow(QObject::tr("Angle"), QStringLiteral("%1°").arg(angle)));
      }
      // Two parallel planes: their distance along the normal (mitcad#646),
      // beside the closest distance, which differs where they do not
      // overlap.
      if (between.value(QStringLiteral("offset")).isDouble()) {
        const QString offset = n(between.value(QStringLiteral("offset")).toDouble());
        lines << QObject::tr("<b>Offset: %1 mm</b>").arg(offset);
        log << QStringLiteral("offset %1").arg(offset);
        result.values.push_back(valueRow(QObject::tr("Offset"), QStringLiteral("%1 mm").arg(offset)));
      }
      const QJsonArray a = between.value(QStringLiteral("on_a")).toArray();
      const QJsonArray c = between.value(QStringLiteral("on_b")).toArray();
      if (a.size() == 3 && c.size() == 3) {
        const gp_Pnt p = pointOf(a);
        const gp_Pnt q = pointOf(c);
        lines << QObject::tr("Δx %1, Δy %2, Δz %3 mm")
                     .arg(n(q.X() - p.X()), n(q.Y() - p.Y()), n(q.Z() - p.Z()));
        lines << QObject::tr("From %1 to %2").arg(xyz(a), xyz(c));
        result.values.front().tip = QObject::tr("From %1 to %2\nΔx %3, Δy %4, Δz %5 mm")
                                        .arg(xyz(a), xyz(c), n(q.X() - p.X()), n(q.Y() - p.Y()), n(q.Z() - p.Z()));
        if (p.Distance(q) > 1e-9) {
          builder.Add(drawn, BRepBuilderAPI_MakeEdge(p, q).Edge());
        }
      }
      // With an axis (mitcad#647): the axes' distance beside the closest
      // one (parallel, skew or meeting axes, an axis and a point or a
      // parallel plane), where it is measured, and two circles' centres.
      if (between.value(QStringLiteral("axis_distance")).isDouble()) {
        const QString distanceText = n(between.value(QStringLiteral("axis_distance")).toDouble());
        lines << QObject::tr("<b>Axis distance: %1 mm</b>").arg(distanceText);
        log << QStringLiteral("axis distance %1").arg(distanceText);
        Inspection::Value axisRow = valueRow(QObject::tr("Axis distance"), QStringLiteral("%1 mm").arg(distanceText));
        const QJsonArray points = between.value(QStringLiteral("axis_points")).toArray();
        if (points.size() == 2) {
          lines << QObject::tr("Axis from %1 to %2").arg(xyz(points.at(0)), xyz(points.at(1)));
          axisRow.tip = QObject::tr("From %1 to %2").arg(xyz(points.at(0)), xyz(points.at(1)));
          const gp_Pnt p = pointOf(points.at(0));
          const gp_Pnt q = pointOf(points.at(1));
          if (p.Distance(q) > 1e-9) {
            builder.Add(drawn, BRepBuilderAPI_MakeEdge(p, q).Edge());
          }
        }
        result.values.push_back(axisRow);
      }
      if (between.value(QStringLiteral("center_distance")).isDouble()) {
        const QString centres = n(between.value(QStringLiteral("center_distance")).toDouble());
        lines << QObject::tr("<b>Centre distance: %1 mm</b>").arg(centres);
        log << QStringLiteral("center distance %1").arg(centres);
        result.values.push_back(valueRow(QObject::tr("Centre distance"), QStringLiteral("%1 mm").arg(centres)));
      }
      parts << lines.join(QStringLiteral("<br>"));
    }
    // Each pick's rows under its kind (mitcad#1010).
    const bool pickBodies = bodies(state);
    result.groups.push_back(pickGroup(1, pickBodies, first.first(), context,
                                      measuredValues(answer.value(QStringLiteral("a")).toObject())));
    if (!second.isEmpty() && answer.contains(QStringLiteral("b"))) {
      result.groups.push_back(pickGroup(2, pickBodies, second.first(), context,
                                        measuredValues(answer.value(QStringLiteral("b")).toObject())));
    }
    if (axes > 0) {
      log << QStringLiteral("axis lines %1").arg(axes);
    }
    TopoDS_Iterator anything(drawn);
    if (anything.More()) {
      result.shape = drawn;
    }
    result.text = parts.join(QStringLiteral("<br><br>"));
    result.log = log.join(QStringLiteral(", "));
    result.record.picked = pickedNames(first.first(), second.isEmpty() ? nullptr : &second.first(), context);
    result.record.values = recordValues(answer);
    result.record.inputs = inputs;
    result.record.main = mainValue(result);
    result.record.kinds = pickedKinds(first.first(), second.isEmpty() ? nullptr : &second.first());
    return result;
  };
  return def;
}

// ---------------------------------------------------------------------------
// Interference

CommandDef interferenceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("inspect.interference");
  def.name = QObject::tr("Interference");
  def.icon = QStringLiteral("interference");
  def.tooltip = QObject::tr("Where bodies overlap, and by how much");
  def.group = QStringLiteral("INSPECT");
  def.keywords = {QStringLiteral("clash"), QStringLiteral("overlap"), QStringLiteral("collision")};
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 0, 0)
          .withTooltip(QObject::tr("The bodies to check; none: all")),
  };
  def.enabled = [&context] { return context.queryArray(QStringLiteral("bodies")).size() > 1; };
  def.inspect = [](const CommandState& state, const CommandContext& model) {
    Inspection result;
    QJsonObject request{{QStringLiteral("query"), QStringLiteral("interference")}};
    const Selection& bodies = state.items(QStringLiteral("bodies"));
    if (!bodies.isEmpty()) {
      request.insert(QStringLiteral("bodies"), bodyUids(bodies));
    }
    const QJsonObject answer = model.queryObject(request);
    const QJsonArray pairs = answer.value(QStringLiteral("pairs")).toArray();
    TopoDS_Compound overlaps;
    BRep_Builder builder;
    builder.MakeCompound(overlaps);
    QStringList rows;
    QStringList log;
    for (const QJsonValue& value : pairs) {
      const QJsonObject pair = value.toObject();
      const QJsonArray names = pair.value(QStringLiteral("names")).toArray();
      const QString volume = n(pair.value(QStringLiteral("volume")).toDouble());
      rows << QStringLiteral("<tr><td>%1</td><td>%2</td><td align='right'>%3 mm³</td></tr>")
                  .arg(names.at(0).toString().toHtmlEscaped(), names.at(1).toString().toHtmlEscaped(), volume);
      log << QStringLiteral("%1 x %2: %3").arg(names.at(0).toString(), names.at(1).toString(), volume);
      const TopoDS_Shape overlap = model.analysisShape({{QStringLiteral("shape"), QStringLiteral("interference")},
                                                        {QStringLiteral("a"), pair.value(QStringLiteral("a"))},
                                                        {QStringLiteral("b"), pair.value(QStringLiteral("b"))}});
      if (!overlap.IsNull()) {
        builder.Add(overlaps, overlap);
      }
    }
    if (pairs.isEmpty()) {
      result.text = QObject::tr("No interference between %1 bodies.").arg(answer.value(QStringLiteral("bodies")).toInt());
      result.log = QStringLiteral("none");
    } else {
      result.text = QObject::tr("%1 interference(s):").arg(pairs.size()) +
                    QStringLiteral("<table width='100%'>%1</table>").arg(rows.join(QString()));
      result.log = log.join(QStringLiteral("; "));
      result.shape = overlaps;
      result.red = true;
    }
    return result;
  };
  return def;
}

// ---------------------------------------------------------------------------
// Section Analysis

// The model's `section_plane` answer for the panel's plane and axis: moved
// by `offset`, flipped with `flip`, and turned by the panel's Angle about
// its Axis when `turned` (mitcad#823); none when it cannot be found.
std::optional<QJsonObject> sectionQuery(const CommandState& state, const CommandContext& context, double offset,
                                        bool flip, bool turned) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (plane.isEmpty() || !std::isfinite(offset)) {
    return std::nullopt;
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("section_plane")},
                      {QStringLiteral("plane"), plane.first().reference()},
                      {QStringLiteral("occurrence"), plane.first().occurrence},
                      {QStringLiteral("offset"), offset},
                      {QStringLiteral("flip"), flip}};
  const Selection& axis = state.items(QStringLiteral("axis"));
  if (!axis.isEmpty()) {
    request.insert(QStringLiteral("axis"), axis.first().reference());
    request.insert(QStringLiteral("axis_occurrence"), axis.first().occurrence);
  }
  if (turned) {
    const double angle = state.value(QStringLiteral("angle"));
    if (!std::isfinite(angle)) {
      return std::nullopt;
    }
    request.insert(QStringLiteral("angle"), angle);
  }
  try {
    return context.queryObject(request);
  } catch (const std::exception&) {
    return std::nullopt;
  }
}

// What the panel says instead of the zero totals when the plane cuts none
// of the bodies shown (mitcad#824), from the `section` query's counts of the
// bodies the clip cuts, keeps whole and removes whole; empty when it cuts
// one, or when the section has an area or an outline after all (a plane on
// a face of a body behind it).
QString sectionMissWarning(const QJsonObject& answer) {
  const int cut = answer.value(QStringLiteral("cut")).toInt();
  const int kept = answer.value(QStringLiteral("kept")).toInt();
  const int removed = answer.value(QStringLiteral("removed")).toInt();
  if (cut > 0 || kept + removed == 0 || answer.value(QStringLiteral("area")).toDouble() > 0.0 ||
      answer.value(QStringLiteral("length")).toDouble() > 0.0) {
    return QString();
  }
  if (kept == 0) {
    return QObject::tr("The plane is beyond the bodies: it cuts them all away. Change Distance or turn on Flip.");
  }
  if (removed == 0) {
    return QObject::tr("The plane is beyond the bodies: it cuts nothing.");
  }
  return QObject::tr("The plane passes between the bodies: it cuts none of them.");
}

// Preview and saved analyses resolve the same occurrence-aware plane in
// the model. The displayed item is in design coordinates, while a datum
// query or a body's underlying shape is in its component's coordinates.
std::optional<std::pair<gp_Pnt, gp_Dir>> sectionPlane(const CommandState& state, const CommandContext& context,
                                                    bool basePlane = false) {
  // The base plane (the arrow's) is the turned plane before Distance and
  // Flip: Distance moves along the normal of the plane shown.
  const double offset = basePlane ? 0.0 : state.value(QStringLiteral("offset"));
  const auto resolved =
      sectionQuery(state, context, offset, !basePlane && state.checked(QStringLiteral("flip")), true);
  if (!resolved) {
    return std::nullopt;
  }
  return std::make_pair(pointOf(resolved->value(QStringLiteral("origin"))),
                        gp_Dir(vectorOf(resolved->value(QStringLiteral("normal")))));
}

// The Angle's ring (mitcad#823): about the axis at its point nearest the
// picked plane's centre, its 0 along the picked plane's normal across the
// axis, so that the knob follows the turned normal.
std::optional<Manipulator> sectionRing(const CommandState& state, const CommandContext& context) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (plane.isEmpty() || state.items(QStringLiteral("axis")).isEmpty()) {
    return std::nullopt;
  }
  const auto picked = sectionQuery(state, context, 0.0, false, false);
  const auto center = centerOf(plane, context);
  if (!picked || !center) {
    return std::nullopt;
  }
  const QJsonObject axis = picked->value(QStringLiteral("axis")).toObject();
  if (axis.isEmpty()) {
    return std::nullopt;
  }
  const gp_Pnt through = pointOf(axis.value(QStringLiteral("origin")));
  const gp_Vec along = gp_Vec(gp_Dir(vectorOf(axis.value(QStringLiteral("direction")))));
  gp_Pnt origin = through;
  origin.Translate(along * gp_Vec(through, *center).Dot(along));
  Manipulator handle = ring(origin, gp_Dir(along));
  gp_Vec normal = vectorOf(picked->value(QStringLiteral("normal")));
  normal -= along * normal.Dot(along);
  if (normal.Magnitude() > 1e-9) {
    handle.reference = gp_Dir(normal);
  }
  return handle;
}

CommandDef sectionCommand() {
  CommandDef def;
  def.id = QStringLiteral("inspect.section");
  def.name = QObject::tr("Section Analysis");
  def.icon = QStringLiteral("section-analysis");
  def.tooltip = QObject::tr("Shows the bodies cut at a plane, with the cut's area");
  def.group = QStringLiteral("INSPECT");
  def.pinned = true;
  def.keywords = {QStringLiteral("cut"), QStringLiteral("clip"), QStringLiteral("cross section")};
  def.keepsSection = true;
  def.inputs = {
      selectionInput(QStringLiteral("plane"), QObject::tr("Faces/Plane"), kPlaneKinds, 1, 1).withAccepts(isPlanar),
      valueInput(QStringLiteral("offset"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withManipulator([](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
            const Selection& plane = state.items(QStringLiteral("plane"));
            if (plane.isEmpty()) {
              return std::nullopt;
            }
            const auto frame = sectionPlane(state, context, true);
            const auto center = centerOf(plane, context);
            if (!frame || !center) {
              return std::nullopt;
            }
            gp_Pnt origin = *center;
            origin.Translate(-gp_Vec(frame->second) * gp_Vec(frame->first, origin).Dot(gp_Vec(frame->second)));
            return arrow(origin, frame->second);
          }),
      // A slanted cut (mitcad#823): the plane turned by Angle about Axis.
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxisKinds, 0, 1)
          .withAccepts(isAxial)
          .withTooltip(QObject::tr("An axis to turn the plane about")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withManipulator(sectionRing),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Cut away the other side")),
  };
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const auto plane = sectionPlane(state, context);
    if (!plane) {
      const double angle = state.value(QStringLiteral("angle"));
      result.error = state.items(QStringLiteral("axis")).isEmpty() && std::isfinite(angle) && angle != 0.0
                         ? QObject::tr("Select an axis for the angle.")
                         : QObject::tr("The plane cannot be found.");
      return result;
    }
    const QJsonObject fixed{{QStringLiteral("origin"), QJsonArray{plane->first.X(), plane->first.Y(), plane->first.Z()}},
                            {QStringLiteral("normal"),
                             QJsonArray{plane->second.X(), plane->second.Y(), plane->second.Z()}}};
    // The view shows the bodies cut at the plane with the cut faces as caps
    // (MainWindow::showSectionCaps, U5); the panel's totals of all bodies
    // are measured after the view has its cut bodies, off the UI thread
    // (mitcad#438: seconds on a large design).
    result.text = QObject::tr("Section area: …<br>Outline length: …");
    result.log = QStringLiteral("measuring");
    result.section = fixed;
    result.laterQuery = {{QStringLiteral("query"), QStringLiteral("section")},
                         {QStringLiteral("plane"), fixed},
                         {QStringLiteral("world"), true}};
    result.measured = [](const QJsonObject& answer, Inspection& shown) {
      const double area = answer.value(QStringLiteral("area")).toDouble();
      const double length = answer.value(QStringLiteral("length")).toDouble();
      // A plane past every body shown (mitcad#824): the view is empty or
      // shows them whole, and the panel says why instead of zero totals.
      const QString warning = sectionMissWarning(answer);
      if (!warning.isEmpty()) {
        shown.text = QStringLiteral("<span style=\"color: %1\">%2</span>")
                         .arg(warningColor().name(), warning.toHtmlEscaped());
        shown.log = warning;
        return;
      }
      shown.text = QObject::tr("Section area: %1 mm²<br>Outline length: %2 mm").arg(n(area), n(length));
      shown.log = QStringLiteral("area %1, length %2").arg(n(area), n(length));
    };
    return result;
  };
  // OK keeps it in the document as an analysis (mitcad#41): the plane as
  // a reference, so that a face's section follows the model.
  def.build = [](const CommandState& state, const CommandContext&) {
    const Selection& plane = state.items(QStringLiteral("plane"));
    if (plane.isEmpty()) {
      return Built::failure(QObject::tr("Select a plane."), QStringLiteral("plane"));
    }
    const double offset = state.value(QStringLiteral("offset"));
    if (!std::isfinite(offset)) {
      return Built::failure(QObject::tr("Distance: check the value."), QStringLiteral("offset"));
    }
    QJsonObject definition{{QStringLiteral("type"), QStringLiteral("section")},
                      {QStringLiteral("plane"), refsOf(plane).first()},
                      {QStringLiteral("offset"), offset},
                      {QStringLiteral("flip"), state.checked(QStringLiteral("flip"))}};
    if (!plane.first().occurrence.isEmpty()) {
      definition.insert(QStringLiteral("occurrence"), plane.first().occurrence);
    }
    // Turned by Angle about Axis (mitcad#823).
    const double angle = state.value(QStringLiteral("angle"));
    if (!std::isfinite(angle)) {
      return Built::failure(QObject::tr("Angle: check the value."), QStringLiteral("angle"));
    }
    const Selection& axis = state.items(QStringLiteral("axis"));
    if (axis.isEmpty() && angle != 0.0) {
      return Built::failure(QObject::tr("Select an axis for the angle."), QStringLiteral("axis"));
    }
    if (!axis.isEmpty()) {
      definition.insert(QStringLiteral("axis"), refsOf(axis).first());
      if (!axis.first().occurrence.isEmpty()) {
        definition.insert(QStringLiteral("axis_occurrence"), axis.first().occurrence);
      }
      definition.insert(QStringLiteral("angle"), angle);
    }
    return Built::of(definition);
  };
  def.load = [](const QJsonObject& analysis, CommandState& state, const CommandContext& context) {
    SelectionItem plane = itemOf(analysis.value(QStringLiteral("plane")), context);
    if (plane.isValid()) {
      if (analysis.value(QStringLiteral("occurrence")).isString()) {
        plane.occurrence = analysis.value(QStringLiteral("occurrence")).toString();
      } else {
        // Older saved analyses had no occurrence. Resolve only an
        // unambiguous component; never silently pick a repeated instance.
        plane.occurrence = context.queryObject({{QStringLiteral("query"), QStringLiteral("section_plane")},
            {QStringLiteral("plane"), analysis.value(QStringLiteral("plane"))}})
                               .value(QStringLiteral("occurrence")).toString();
      }
      state.items(QStringLiteral("plane")) = {plane};
    }
    state.setText(QStringLiteral("offset"), lengthText(analysis.value(QStringLiteral("offset")).toDouble(), context));
    state.setChecked(QStringLiteral("flip"), analysis.value(QStringLiteral("flip")).toBool());
    // The axis and angle of a turned section (mitcad#823).
    SelectionItem axis = itemOf(analysis.value(QStringLiteral("axis")), context);
    if (axis.isValid()) {
      if (analysis.value(QStringLiteral("axis_occurrence")).isString()) {
        axis.occurrence = analysis.value(QStringLiteral("axis_occurrence")).toString();
      } else {
        // Written without a path: the model's (the root, or the axis'
        // component's only placement).
        QJsonObject request{{QStringLiteral("query"), QStringLiteral("section_plane")},
                            {QStringLiteral("plane"), analysis.value(QStringLiteral("plane"))},
                            {QStringLiteral("axis"), analysis.value(QStringLiteral("axis"))}};
        if (analysis.value(QStringLiteral("occurrence")).isString()) {
          request.insert(QStringLiteral("occurrence"), analysis.value(QStringLiteral("occurrence")));
        }
        try {
          axis.occurrence = context.queryObject(request).value(QStringLiteral("axis_occurrence")).toString();
        } catch (const std::exception&) {
          // Not found at the marker: the axis as it was picked.
        }
      }
      state.items(QStringLiteral("axis")) = {axis};
    }
    const double angle = qRadiansToDegrees(analysis.value(QStringLiteral("angle")).toDouble());
    state.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(n(angle, 9)));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Print Analysis (mitcad#620)

// Solids and mesh bodies can be printed (as 3D Print takes them); sheet and
// empty bodies cannot.
bool printable(const std::shared_ptr<geometry::Shape>& shape) {
  const geometry::BodyKind kind = shape ? geometry::body_kind(shape->occt()) : geometry::BodyKind::Empty;
  return kind == geometry::BodyKind::Solid || kind == geometry::BodyKind::Mesh;
}

// The Material row: the bodies' own (PLA for a body without one), or one
// of the physical materials.
QVector<QPair<QString, QString>> printMaterials() {
  QVector<QPair<QString, QString>> choices{{QStringLiteral("bodies"), QObject::tr("The bodies' material (else PLA)")}};
  choices += physicalMaterials();
  return choices;
}

// The panel's inputs from settings: the settings group's (a new analysis)
// or an analysis' own (its definition).
void setPrintSettings(CommandState& state, const PrintAnalysisSettings& s, const CommandContext& context) {
  state.setChecked(QStringLiteral("overhang"), s.overhang);
  state.setText(QStringLiteral("angle"), QStringLiteral("%1 deg").arg(n(s.angle)));
  state.setChecked(QStringLiteral("thin_wall"), s.thinWall);
  state.setText(QStringLiteral("minimum"), lengthText(s.minimum, context));
  state.setChecked(QStringLiteral("estimate"), s.estimate);
  state.setChoice(QStringLiteral("material"), s.material.isEmpty() ? QStringLiteral("bodies") : s.material);
  state.setText(QStringLiteral("line_width"), lengthText(s.lineWidth, context));
  state.setText(QStringLiteral("walls"), QString::number(s.walls));
  state.setText(QStringLiteral("layer_height"), lengthText(s.layerHeight, context));
  state.setText(QStringLiteral("top_bottom_layers"), QString::number(s.topBottomLayers));
  state.setText(QStringLiteral("infill"), n(s.infill * 100.0));
  state.setText(QStringLiteral("filament_diameter"), lengthText(s.filamentDiameter, context));
  state.setText(QStringLiteral("price"), s.pricePerKg ? n(*s.pricePerKg, 2) : QString());
}

// The panel's values as an analysis' definition, or what is wrong (and in
// which input). Values of a part switched off are not checked: they keep
// the defaults when they cannot be read.
Built printDefinition(const CommandState& state) {
  PrintAnalysisSettings s;
  const auto valueOf = [&state](const char* key) { return state.value(QLatin1String(key)); };
  const auto whole = [](double value) { return value >= 0.0 && value <= 1000.0 && std::floor(value) == value; };
  s.overhang = state.checked(QStringLiteral("overhang"));
  if (s.overhang) {
    const double angle = qRadiansToDegrees(valueOf("angle"));
    if (!(angle >= 0.0 && angle <= 90.0)) {
      return Built::failure(QObject::tr("Angle: between 0 and 90 deg."), QStringLiteral("angle"));
    }
    s.angle = angle;
  }
  s.thinWall = state.checked(QStringLiteral("thin_wall"));
  if (s.thinWall) {
    const double minimum = valueOf("minimum");
    if (!(minimum > 0.0 && minimum <= 100.0)) {
      return Built::failure(QObject::tr("Minimum wall: more than 0 mm."), QStringLiteral("minimum"));
    }
    s.minimum = minimum;
  }
  s.estimate = state.checked(QStringLiteral("estimate"));
  if (s.estimate) {
    const QString material = state.choice(QStringLiteral("material"));
    s.material = material == QStringLiteral("bodies") ? QString() : material;
    const double lineWidth = valueOf("line_width");
    if (!(lineWidth > 0.0 && lineWidth <= 100.0)) {
      return Built::failure(QObject::tr("Line width: more than 0 mm."), QStringLiteral("line_width"));
    }
    s.lineWidth = lineWidth;
    const double walls = valueOf("walls");
    if (!whole(walls)) {
      return Built::failure(QObject::tr("Walls: a whole number, 0 or more."), QStringLiteral("walls"));
    }
    s.walls = static_cast<int>(walls);
    const double layerHeight = valueOf("layer_height");
    if (!(layerHeight > 0.0 && layerHeight <= 100.0)) {
      return Built::failure(QObject::tr("Layer height: more than 0 mm."), QStringLiteral("layer_height"));
    }
    s.layerHeight = layerHeight;
    const double layers = valueOf("top_bottom_layers");
    if (!whole(layers)) {
      return Built::failure(QObject::tr("Top/bottom layers: a whole number, 0 or more."),
                            QStringLiteral("top_bottom_layers"));
    }
    s.topBottomLayers = static_cast<int>(layers);
    const double infill = valueOf("infill");
    if (!(infill >= 0.0 && infill <= 100.0)) {
      return Built::failure(QObject::tr("Infill: between 0 and 100 %."), QStringLiteral("infill"));
    }
    s.infill = infill / 100.0;
    const double diameter = valueOf("filament_diameter");
    if (!(diameter > 0.0 && diameter <= 100.0)) {
      return Built::failure(QObject::tr("Filament diameter: more than 0 mm."), QStringLiteral("filament_diameter"));
    }
    s.filamentDiameter = diameter;
    const QString price = state.text(QStringLiteral("price")).trimmed();
    if (!price.isEmpty()) {
      const std::optional<double> perKg = parsePlainNumber(price);
      if (!perKg || !std::isfinite(*perKg) || *perKg < 0.0) {
        return Built::failure(QObject::tr("Price: a number, or empty for no cost."), QStringLiteral("price"));
      }
      s.pricePerKg = *perKg;
    }
  }
  // Each body once, in the order picked (a body shown more than once is
  // one body of the analysis).
  QJsonArray bodies;
  for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
    if (!bodies.contains(item.owner)) {
      bodies.append(item.owner);
    }
  }
  if (bodies.isEmpty()) {
    return Built::failure(QObject::tr("Select the bodies."), QStringLiteral("bodies"));
  }
  const Selection& bed = state.items(QStringLiteral("bed_face"));
  if (!bed.isEmpty() && !bodies.contains(bed.first().owner)) {
    return Built::failure(QObject::tr("The bed face is not a face of the bodies."), QStringLiteral("bed_face"));
  }
  return Built::of(s.def(bodies, bed.isEmpty() ? QJsonValue() : bed.first().reference()));
}

// What a long query's progress says.
QString printStage(const QJsonObject& request) {
  return request.value(QStringLiteral("query")).toString() == QStringLiteral("print_thickness")
             ? QObject::tr("Measuring the walls")
             : QObject::tr("Finding the overhangs");
}

// Overhangs and thickness, which may take long on large meshes: in the
// window as a model job on the model's worker, with the progress and its
// Cancel (CommandHost::jobQuery); an inspection of the native window runs on
// the worker already and asks the document as it is.
QJsonObject printQuery(const CommandContext& context, const QJsonObject& request, bool slow) {
  if (slow) {
    if (const auto* host = dynamic_cast<const CommandHost*>(&context)) {
      return const_cast<CommandHost*>(host)->jobQuery(request, printStage(request));
    }
  }
  return context.queryObject(request);
}

// The bed face a new analysis starts from (mitcad#731), by the export's
// rule: the print bed face of the one analysed body that has a usable one,
// in the occurrence the body was taken in; invalid when none or several
// bodies have one.
SelectionItem storedBedFace(const Selection& bodies, const CommandContext& context) {
  const QJsonArray all = context.queryArray(QStringLiteral("bodies"));
  QStringList seen;
  SelectionItem found;
  int usable = 0;
  for (const SelectionItem& body : bodies) {
    if (seen.contains(body.owner)) {
      continue; // a body shown more than once is one body of the analysis
    }
    seen << body.owner;
    for (const QJsonValue& value : all) {
      const QJsonObject entry = value.toObject();
      if (entry.value(QStringLiteral("uid")).toString() != body.owner) {
        continue;
      }
      const QJsonObject bed = entry.value(QStringLiteral("print_bed_face")).toObject();
      const QJsonArray faces = bed.value(QStringLiteral("faces")).toArray();
      if (!bed.contains(QStringLiteral("error")) && !faces.isEmpty() && ++usable == 1) {
        found =
            itemOf(QJsonObject{{QStringLiteral("body"), body.owner}, {QStringLiteral("face"), faces.first()}}, context);
        found.occurrence = body.occurrence;
      }
      break;
    }
  }
  // Several bodies with bed faces: kept as assembled, so none.
  return usable == 1 && isPlanar(found) ? found : SelectionItem();
}

CommandDef printAnalysisCommand(const CommandContext& model) {
  CommandDef def;
  def.id = QStringLiteral("inspect.print_analysis");
  def.name = QObject::tr("Print Analysis");
  def.icon = QStringLiteral("print-analysis");
  def.tooltip = QObject::tr("Overhangs, thin walls and an estimate of weight, filament and cost");
  def.group = QStringLiteral("INSPECT");
  def.keywords = {QStringLiteral("3d print"), QStringLiteral("overhang"), QStringLiteral("thin wall"),
                  QStringLiteral("filament"), QStringLiteral("weight"), QStringLiteral("support"),
                  QStringLiteral("print estimate")};
  // OK keeps it in the document as an analysis, shown (mitcad#41).
  def.keepsSection = true;
  const auto checked = [](const QString& key) {
    return [key](const CommandState& state) { return state.checked(key); };
  };
  const auto overhang = checked(QStringLiteral("overhang"));
  const auto thin = checked(QStringLiteral("thin_wall"));
  const auto estimate = checked(QStringLiteral("estimate"));
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 1, 0)
          .withAccepts([&model](const SelectionItem& item) { return printable(model.bodyShape(item.owner)); })
          .withTooltip(QObject::tr("The bodies to check: a click on a body adds or removes it")),
      selectionInput(QStringLiteral("bed_face"), QObject::tr("Bed face"), SelectKind::Face, 0, 1)
          .withAccepts(isPlanar)
          .withTooltip(QObject::tr("The planar face that lies on the bed; none: -Z is down")),
      checkInput(QStringLiteral("overhang"), QObject::tr("Overhangs"), true)
          .withTooltip(QObject::tr("Faces that face down more steeply than the angle need support")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("45 deg"))
          .withVisible(overhang)
          .withTooltip(QObject::tr("The steepest overhang the printer makes without support, from the vertical")),
      checkInput(QStringLiteral("thin_wall"), QObject::tr("Thin walls"), true)
          .withTooltip(QObject::tr("Walls thinner than the printer can make")),
      valueInput(QStringLiteral("minimum"), QObject::tr("Minimum wall"), ValueKind::Length, QStringLiteral("0.8 mm"))
          .withVisible(thin)
          .withTooltip(QObject::tr("The thinnest wall the printer makes: about twice the line width")),
      checkInput(QStringLiteral("estimate"), QObject::tr("Estimate"), true)
          .withTooltip(QObject::tr("About the weight, filament length and cost of the print")),
      choiceInput(QStringLiteral("material"), QObject::tr("Material"), printMaterials(), QStringLiteral("bodies"))
          .withVisible(estimate),
      valueInput(QStringLiteral("line_width"), QObject::tr("Line width"), ValueKind::Length, QStringLiteral("0.4 mm"))
          .withVisible(estimate),
      valueInput(QStringLiteral("walls"), QObject::tr("Walls"), ValueKind::Unitless, QStringLiteral("2"))
          .withVisible(estimate)
          .withTooltip(QObject::tr("The perimeters around each layer")),
      valueInput(QStringLiteral("layer_height"), QObject::tr("Layer height"), ValueKind::Length,
                 QStringLiteral("0.2 mm"))
          .withVisible(estimate),
      valueInput(QStringLiteral("top_bottom_layers"), QObject::tr("Top/bottom layers"), ValueKind::Unitless,
                 QStringLiteral("4"))
          .withVisible(estimate),
      valueInput(QStringLiteral("infill"), QObject::tr("Infill %"), ValueKind::Unitless, QStringLiteral("15"))
          .withVisible(estimate)
          .withTooltip(QObject::tr("How much of the inside is filled, in per cent")),
      valueInput(QStringLiteral("filament_diameter"), QObject::tr("Filament diameter"), ValueKind::Length,
                 QStringLiteral("1.75 mm"))
          .withVisible(estimate),
      textInput(QStringLiteral("price"), QObject::tr("Price €/kg"))
          .withVisible(estimate)
          .withTooltip(QObject::tr("The filament's price per kilogram; empty: no cost")),
  };
  for (InputDef& input : def.inputs) {
    input.showsOrigin = false;
  }
  // What the selection stands for (a face, edge or vertex its body, an
  // occurrence the bodies shown in it), else every shown body; each body
  // once, sheets and empty bodies left out (as 3D Print starts).
  def.preselect = [](const Selection& before, CommandState& state, const CommandContext& context) {
    bool selected = false;
    const Selection start = printStartBodies(before, context.queryArray(QStringLiteral("instances")), &selected);
    Selection bodies;
    QStringList seen;
    for (const SelectionItem& item : start) {
      if (seen.contains(item.owner)) {
        continue;
      }
      seen << item.owner;
      if (printable(context.bodyShape(item.owner))) {
        bodies.append(item);
      }
    }
    state.setItems(QStringLiteral("bodies"), bodies);
    // Printed on the bodies' print bed face (mitcad#731), unless another
    // is picked.
    const SelectionItem bed = storedBedFace(bodies, context);
    if (bed.isValid()) {
      state.items(QStringLiteral("bed_face")) = {bed};
      qDebug().noquote() << QStringLiteral("Print Analysis bed face: %1/%2").arg(bed.owner, bed.name);
    }
  };
  // A new analysis starts with the settings group's numbers.
  def.init = [](CommandState& state, const CommandContext& context) {
    QSettings settings;
    setPrintSettings(state, PrintAnalysisSettings::load(settings), context);
  };
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const Built built = printDefinition(state);
    if (!built.error.isEmpty()) {
      result.error = built.error;
      return result;
    }
    PrintAnswers answers;
    const QString error = runPrintAnalysis(
        built.def,
        [&context](const QJsonObject& request, bool slow) { return printQuery(context, request, slow); }, answers);
    if (!error.isEmpty()) {
      result.error = error;
      return result;
    }
    const BodyNames name = [&context](const QString& uid) { return context.bodyName(uid); };
    result.text = printAnalysisText(answers, name);
    result.log = printAnalysisLog(answers, name);
    // Drawn over the bodies (CommandHost::showPrintAnalysis).
    result.printOverlay = printOverlayJson(built.def, answers, QObject::tr("Print Analysis"));
    return result;
  };
  // OK keeps it (add_analysis, or edit_analysis of the one edited), and the
  // numbers become the settings group's for the next one. An inspection's
  // `build` runs only then (CommandSession::keepAnalysis).
  def.build = [](const CommandState& state, const CommandContext&) {
    const Built built = printDefinition(state);
    if (built.error.isEmpty()) {
      QSettings settings;
      PrintAnalysisSettings::fromDef(built.def).save(settings);
    }
    return built;
  };
  def.load = [](const QJsonObject& analysis, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(analysis.value(QStringLiteral("bodies")).toArray()));
    const QJsonValue bed = analysis.value(QStringLiteral("bed_face"));
    if (bed.isObject() || bed.isString()) {
      const SelectionItem face = itemOf(bed, context);
      if (face.isValid()) {
        state.items(QStringLiteral("bed_face")) = {face};
      }
    }
    setPrintSettings(state, PrintAnalysisSettings::fromDef(analysis), context);
  };
  return def;
}

// ---------------------------------------------------------------------------
// Physical Properties

Inspection propertiesInspection(const QJsonObject& answer) {
  // Keep the report's semantics across both frontends: a section heading
  // or a label/value row. Flattening an HTML table loses its columns and
  // makes all native text inherit the headings' emphasis.
  QJsonArray rows;
  const auto heading = [&rows](const QString& key, const QString& text) {
    rows.append(QJsonObject{{QStringLiteral("key"), key}, {QStringLiteral("heading"), text}});
  };
  const auto row = [&rows](const QString& key, const QString& label, const QString& value) {
    rows.append(QJsonObject{{QStringLiteral("key"), key}, {QStringLiteral("label"), label},
                            {QStringLiteral("value"), value}});
  };
  QStringList log;
  int index = 0;
  for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    const QString name = body.value(QStringLiteral("name")).toString();
    const QString prefix = QStringLiteral("propertiesBody%1_").arg(index++);
    heading(prefix + QStringLiteral("heading"), name);
    row(prefix + QStringLiteral("material"), QObject::tr("Material"),
        body.value(QStringLiteral("material")).toString(QStringLiteral("steel")));
    row(prefix + QStringLiteral("density"), QObject::tr("Density"),
        QStringLiteral("%1 g/cm³").arg(n(body.value(QStringLiteral("density")).toDouble(), 4)));
    row(prefix + QStringLiteral("mass"), QObject::tr("Mass"),
        QStringLiteral("%1 kg").arg(n(body.value(QStringLiteral("mass")).toDouble(), 6)));
    row(prefix + QStringLiteral("volume"), QObject::tr("Volume"),
        QStringLiteral("%1 mm³").arg(n(body.value(QStringLiteral("volume")).toDouble())));
    row(prefix + QStringLiteral("area"), QObject::tr("Area"),
        QStringLiteral("%1 mm²").arg(n(body.value(QStringLiteral("area")).toDouble())));
    row(prefix + QStringLiteral("center"), QObject::tr("Center of Mass"),
        xyz(body.value(QStringLiteral("center_of_mass"))) + QStringLiteral(" mm"));
    const QJsonArray moments = body.value(QStringLiteral("principal_moments")).toArray();
    if (moments.size() == 3) {
      row(prefix + QStringLiteral("moments"), QObject::tr("Principal Moments"),
          QStringLiteral("%1, %2, %3 kg mm²")
              .arg(n(moments[0].toDouble()), n(moments[1].toDouble()), n(moments[2].toDouble())));
    }
    log << QStringLiteral("%1 volume %2 mass %3 center %4")
               .arg(name, n(body.value(QStringLiteral("volume")).toDouble()),
                    n(body.value(QStringLiteral("mass")).toDouble(), 6),
                    xyz(body.value(QStringLiteral("center_of_mass"))));
  }
  const QJsonObject total = answer.value(QStringLiteral("total")).toObject();
  heading(QStringLiteral("propertiesTotal_heading"), QObject::tr("Total"));
  row(QStringLiteral("propertiesTotal_mass"), QObject::tr("Mass"),
      QStringLiteral("%1 kg").arg(n(total.value(QStringLiteral("mass")).toDouble(), 6)));
  row(QStringLiteral("propertiesTotal_volume"), QObject::tr("Volume"),
      QStringLiteral("%1 mm³").arg(n(total.value(QStringLiteral("volume")).toDouble())));
  row(QStringLiteral("propertiesTotal_center"), QObject::tr("Center of Mass"),
      xyz(total.value(QStringLiteral("center_of_mass"))) + QStringLiteral(" mm"));
  Inspection result;
  result.report.insert(QStringLiteral("property_rows"), rows);
  result.log = log.join(QStringLiteral("; "));
  return result;
}

void presentProperties(const Inspection& result, QWidget* window) {
  qDebug().noquote() << QStringLiteral("Physical properties: %1").arg(result.log);
  QDialog dialog(window);
  dialog.setObjectName(QStringLiteral("physicalPropertiesDialog"));
  dialog.setWindowTitle(QObject::tr("Properties"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* report = new QWidget;
  report->setObjectName(QStringLiteral("physicalPropertiesReport"));
  auto* form = new QFormLayout(report);
  form->setContentsMargins(4, 4, 4, 4);
  form->setHorizontalSpacing(12);
  form->setVerticalSpacing(4);
  form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
  for (const QJsonValue& entry : result.report.value(QStringLiteral("property_rows")).toArray()) {
    const QJsonObject row = entry.toObject();
    const QString key = row.value(QStringLiteral("key")).toString();
    if (row.contains(QStringLiteral("heading"))) {
      auto* heading = new QLabel(QStringLiteral("<b>%1</b>")
                                     .arg(row.value(QStringLiteral("heading")).toString().toHtmlEscaped()));
      heading->setObjectName(key);
      heading->setTextFormat(Qt::RichText);
      form->addRow(heading);
      continue;
    }
    const QString caption = row.value(QStringLiteral("label")).toString();
    auto* value = new QLabel(row.value(QStringLiteral("value")).toString());
    value->setObjectName(key);
    value->setTextFormat(Qt::PlainText);
    value->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::TextSelectableByKeyboard);
    value->setFocusPolicy(Qt::StrongFocus);
    value->setWordWrap(true);
    value->setAccessibleName(caption);
    auto* label = new QLabel(caption);
    label->setObjectName(key + QStringLiteral("_label"));
    label->setTextFormat(Qt::PlainText);
    label->setBuddy(value);
    form->addRow(label, value);
  }
  auto* scroll = new QScrollArea;
  scroll->setObjectName(QStringLiteral("physicalPropertiesScroll"));
  scroll->setWidget(report);
  scroll->setWidgetResizable(true);
  layout->addWidget(scroll);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(420, 380);
  prepareModal(&dialog);
  execDialog(&dialog);
}

void showProperties(const CommandContext& context, QWidget* window, const Selection& selection) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("properties")}};
  QJsonArray uids;
  for (const SelectionItem& item : selection) {
    const bool onBody = item.kind == SelectKind::Body || item.kind == SelectKind::Face ||
                        item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
    if (onBody && !uids.contains(item.owner)) {
      uids.append(item.owner);
    }
  }
  if (!uids.isEmpty()) {
    request.insert(QStringLiteral("bodies"), uids);
  }
  auto* host = dynamic_cast<CommandHost*>(const_cast<CommandContext*>(&context));
  if (host && host->asynchronousCommands()) {
    host->modelInspectionAsync(QObject::tr("Physical Properties"), window,
        [request](const Document& document) {
          return propertiesInspection(parseObject(document.query(rustStr(compactJson(request)))));
        }, [window](const Inspection& result, const CommandHost::AsyncResult& outcome) {
          if (outcome.succeeded()) {
            presentProperties(result, window);
          } else if (!outcome.cancelled) {
            qWarning().noquote() << "Physical properties:" << outcome.error;
          }
        });
    return;
  }
  try {
    presentProperties(propertiesInspection(context.queryObject(request)), window);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Physical properties:" << e.what();
  }
}

} // namespace

void registerInspectCommands(CommandRegistry& registry, const CommandContext& context,
                             QWidget* window, std::function<Selection()> selection) {
  registry.add(measureCommand());
  registry.add(interferenceCommand(context));
  registry.add(sectionCommand());
  registry.add(printAnalysisCommand(context));

  CommandDef properties;
  properties.id = QStringLiteral("inspect.properties");
  properties.name = QObject::tr("Physical Properties");
  properties.icon = QStringLiteral("properties");
  properties.tooltip = QObject::tr("Mass, volume, area and centre of mass of the selected bodies (all when none)");
  properties.group = QStringLiteral("INSPECT");
  properties.kind = CommandDef::Kind::Action;
  properties.keywords = {QStringLiteral("mass"), QStringLiteral("weight"), QStringLiteral("center of mass"),
                         QStringLiteral("inertia")};
  properties.enabled = [&context] { return hasBodies(context); };
  properties.run = [&context, window, selection] { showProperties(context, window, selection()); };
  registry.add(properties);
}

} // namespace mitcad
