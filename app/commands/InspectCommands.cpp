// SPDX-License-Identifier: MIT
// The INSPECT group of the SOLID tab (U4): Measure, Interference, Section
// Analysis and Physical Properties, on the model's analysis queries
// (commands.md, "Analysis queries").
// They show answers; only Section Analysis leaves something behind: the
// bodies stay cut at its plane until Remove Section Analysis.
#include "Commands.hpp"

#include <algorithm>
#include <cmath>
#include <optional>

#include <QDialog>
#include <QDialogButtonBox>
#include <QJsonArray>
#include <QKeySequence>
#include <QLabel>
#include <QScrollArea>
#include <QVBoxLayout>
#include <QtLogging>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandSupport.hpp"

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
    lines << QObject::tr("Radius: %1 mm, centre %2")
                 .arg(n(circle.value(QStringLiteral("radius")).toDouble()), xyz(circle.value(QStringLiteral("center"))));
    log << QStringLiteral("radius %1").arg(n(circle.value(QStringLiteral("radius")).toDouble()));
  }
  return lines.join(QStringLiteral("<br>"));
}

QJsonValue measureRef(const SelectionItem& item) {
  if (item.kind == SelectKind::Plane || item.kind == SelectKind::Axis || item.kind == SelectKind::Point) {
    return QJsonObject{{QStringLiteral("datum"), item.owner}};
  }
  return item.reference();
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
      lines << QObject::tr("Radius: %1 mm, centre %2").arg(n(circle.Radius()), pointText(circle.Location()));
      log << QStringLiteral("radius %1").arg(n(circle.Radius()));
    }
  }
  return lines.join(QStringLiteral("<br>"));
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
  if (second != nullptr) {
    parts << describeShape(*second, b, log);
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
    }
    const auto da = lineDirection(a);
    const auto db = lineDirection(b);
    if (da && db) {
      // The angle between the lines, 0 to 90 degrees.
      const QString angle = degrees(std::acos(std::min(1.0, std::abs(da->Dot(*db)))));
      lines << QObject::tr("<b>Angle: %1°</b>").arg(angle);
      log << QStringLiteral("angle %1 deg").arg(angle);
    }
    parts << lines.join(QStringLiteral("<br>"));
  }
  result.text = parts.join(QStringLiteral("<hr>"));
  result.log = log.join(QStringLiteral(", "));
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
  // the sketch.
  def.mode = CommandDef::Mode::Any;
  const SelectFilter kinds = SelectKind::Face | SelectKind::Edge | SelectKind::Vertex |
                             SelectKind::Plane | SelectKind::Axis | SelectKind::Point |
                             SelectKind::SketchCurve | SelectKind::SketchPoint;
  const auto bodies = [](const CommandState& s) {
    return s.choice(QStringLiteral("filter")) == QStringLiteral("bodies");
  };
  const auto geometry = [bodies](const CommandState& s) { return !bodies(s); };
  def.inputs = {
      choiceInput(QStringLiteral("filter"), QObject::tr("Selection Filter"),
                  {{QStringLiteral("geometry"), QObject::tr("Faces, Edges and Vertices")},
                   {QStringLiteral("bodies"), QObject::tr("Bodies")}},
                  QStringLiteral("geometry")),
      selectionInput(QStringLiteral("a"), QObject::tr("Selection 1"), kinds, 1, 1).withVisible(geometry),
      selectionInput(QStringLiteral("b"), QObject::tr("Selection 2"), kinds, 0, 1).withVisible(geometry),
      selectionInput(QStringLiteral("body_a"), QObject::tr("Body 1"), SelectKind::Body, 1, 1).withVisible(bodies),
      selectionInput(QStringLiteral("body_b"), QObject::tr("Body 2"), SelectKind::Body, 0, 1).withVisible(bodies),
  };
  for (InputDef& input : def.inputs) {
    input.showsOrigin = false;
  }
  def.inspect = [bodies](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const bool whole = bodies(state);
    const Selection& first = state.items(whole ? QStringLiteral("body_a") : QStringLiteral("a"));
    const Selection& second = state.items(whole ? QStringLiteral("body_b") : QStringLiteral("b"));
    if (isSketchItem(first.first()) || (!second.isEmpty() && isSketchItem(second.first()))) {
      return measureShapes(first.first(), second.isEmpty() ? nullptr : &second.first(), context);
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
    const QJsonObject between = answer.value(QStringLiteral("between")).toObject();
    if (!between.isEmpty()) {
      QStringList lines;
      const double distance = between.value(QStringLiteral("distance")).toDouble();
      lines << QObject::tr("<b>Distance: %1 mm</b>").arg(n(distance));
      log << QStringLiteral("distance %1").arg(n(distance));
      if (between.value(QStringLiteral("angle")).isDouble()) {
        const QString angle = degrees(between.value(QStringLiteral("angle")).toDouble());
        lines << QObject::tr("<b>Angle: %1°</b>").arg(angle);
        log << QStringLiteral("angle %1 deg").arg(angle);
      }
      const QJsonArray a = between.value(QStringLiteral("on_a")).toArray();
      const QJsonArray c = between.value(QStringLiteral("on_b")).toArray();
      if (a.size() == 3 && c.size() == 3) {
        const gp_Pnt p(a[0].toDouble(), a[1].toDouble(), a[2].toDouble());
        const gp_Pnt q(c[0].toDouble(), c[1].toDouble(), c[2].toDouble());
        lines << QObject::tr("Δx %1, Δy %2, Δz %3 mm")
                     .arg(n(q.X() - p.X()), n(q.Y() - p.Y()), n(q.Z() - p.Z()));
        lines << QObject::tr("From %1 to %2").arg(xyz(a), xyz(c));
        if (p.Distance(q) > 1e-9) {
          result.shape = BRepBuilderAPI_MakeEdge(p, q).Edge();
        }
      }
      parts << lines.join(QStringLiteral("<br>"));
    }
    result.text = parts.join(QStringLiteral("<hr>"));
    result.log = log.join(QStringLiteral(", "));
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

// The section plane: the plane picked, moved by the offset, its normal
// turned round with Flip (the side that is cut away).
std::optional<std::pair<gp_Pnt, gp_Dir>> sectionPlane(const CommandState& state, const CommandContext& context) {
  const Selection& plane = state.items(QStringLiteral("plane"));
  if (plane.isEmpty()) {
    return std::nullopt;
  }
  const auto frame = frameOf(plane.first(), context);
  const auto center = centerOf(plane, context);
  const double offset = state.value(QStringLiteral("offset"));
  if (!frame || !center || !std::isfinite(offset)) {
    return std::nullopt;
  }
  // The middle of the face or datum, on its plane.
  gp_Pnt origin = *center;
  origin.Translate(-gp_Vec(frame->normal) * gp_Vec(frame->origin, origin).Dot(gp_Vec(frame->normal)));
  origin.Translate(gp_Vec(frame->normal) * offset);
  gp_Dir normal = frame->normal;
  if (state.checked(QStringLiteral("flip"))) {
    normal.Reverse();
  }
  return std::make_pair(origin, normal);
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
            const auto frame = frameOf(plane.first(), context);
            const auto center = centerOf(plane, context);
            if (!frame || !center) {
              return std::nullopt;
            }
            gp_Pnt origin = *center;
            origin.Translate(-gp_Vec(frame->normal) * gp_Vec(frame->origin, origin).Dot(gp_Vec(frame->normal)));
            return arrow(origin, frame->normal);
          }),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")).withTooltip(QObject::tr("Cut away the other side")),
  };
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    const auto plane = sectionPlane(state, context);
    if (!plane) {
      result.error = QObject::tr("The plane cannot be found.");
      return result;
    }
    const QJsonObject fixed{{QStringLiteral("origin"), QJsonArray{plane->first.X(), plane->first.Y(), plane->first.Z()}},
                            {QStringLiteral("normal"),
                             QJsonArray{plane->second.X(), plane->second.Y(), plane->second.Z()}}};
    const QJsonObject answer =
        context.queryObject({{QStringLiteral("query"), QStringLiteral("section")}, {QStringLiteral("plane"), fixed}});
    // The totals: of all bodies. The view shows the bodies cut at the plane
    // with the cut faces as caps (MainWindow::showSectionCaps, U5).
    const double area = answer.value(QStringLiteral("area")).toDouble();
    const double length = answer.value(QStringLiteral("length")).toDouble();
    result.text = QObject::tr("Section area: %1 mm²<br>Outline length: %2 mm").arg(n(area), n(length));
    result.log = QStringLiteral("area %1, length %2").arg(n(area), n(length));
    result.section = fixed;
    return result;
  };
  return def;
}

// ---------------------------------------------------------------------------
// Physical Properties

void showProperties(const CommandContext& context, QWidget* window, const Selection& selection) {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("properties")}};
  QJsonArray uids;
  for (const SelectionItem& item : selection) {
    // A face, edge or vertex stands for its body.
    const bool onBody = item.kind == SelectKind::Body || item.kind == SelectKind::Face ||
                        item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
    if (onBody && !uids.contains(item.owner)) {
      uids.append(item.owner);
    }
  }
  if (!uids.isEmpty()) {
    request.insert(QStringLiteral("bodies"), uids);
  }
  QJsonObject answer;
  try {
    answer = context.queryObject(request);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Physical properties:" << e.what();
    return;
  }
  QString html = QStringLiteral("<table cellspacing='4'>");
  const auto row = [&html](const QString& label, const QString& value) {
    html += QStringLiteral("<tr><td>%1</td><td>%2</td></tr>").arg(label, value);
  };
  QStringList log;
  for (const QJsonValue& value : answer.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    const QString name = body.value(QStringLiteral("name")).toString();
    html += QStringLiteral("<tr><td colspan='2'><b>%1</b></td></tr>").arg(name.toHtmlEscaped());
    row(QObject::tr("Material"), body.value(QStringLiteral("material")).toString(QStringLiteral("steel")));
    row(QObject::tr("Density"), QStringLiteral("%1 g/cm³").arg(n(body.value(QStringLiteral("density")).toDouble(), 4)));
    row(QObject::tr("Mass"), QStringLiteral("%1 kg").arg(n(body.value(QStringLiteral("mass")).toDouble(), 6)));
    row(QObject::tr("Volume"), QStringLiteral("%1 mm³").arg(n(body.value(QStringLiteral("volume")).toDouble())));
    row(QObject::tr("Area"), QStringLiteral("%1 mm²").arg(n(body.value(QStringLiteral("area")).toDouble())));
    row(QObject::tr("Center of Mass"), xyz(body.value(QStringLiteral("center_of_mass"))) + QStringLiteral(" mm"));
    const QJsonArray moments = body.value(QStringLiteral("principal_moments")).toArray();
    if (moments.size() == 3) {
      row(QObject::tr("Principal Moments"), QStringLiteral("%1, %2, %3 kg mm²")
                                               .arg(n(moments[0].toDouble()), n(moments[1].toDouble()),
                                                    n(moments[2].toDouble())));
    }
    log << QStringLiteral("%1 volume %2 mass %3 center %4")
               .arg(name, n(body.value(QStringLiteral("volume")).toDouble()),
                    n(body.value(QStringLiteral("mass")).toDouble(), 6),
                    xyz(body.value(QStringLiteral("center_of_mass"))));
  }
  const QJsonObject total = answer.value(QStringLiteral("total")).toObject();
  html += QStringLiteral("<tr><td colspan='2'><b>%1</b></td></tr>").arg(QObject::tr("Total"));
  row(QObject::tr("Mass"), QStringLiteral("%1 kg").arg(n(total.value(QStringLiteral("mass")).toDouble(), 6)));
  row(QObject::tr("Volume"), QStringLiteral("%1 mm³").arg(n(total.value(QStringLiteral("volume")).toDouble())));
  row(QObject::tr("Center of Mass"), xyz(total.value(QStringLiteral("center_of_mass"))) + QStringLiteral(" mm"));
  html += QStringLiteral("</table>");
  qDebug().noquote() << QStringLiteral("Physical properties: %1").arg(log.join(QStringLiteral("; ")));

  QDialog dialog(window);
  dialog.setWindowTitle(QObject::tr("Properties"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* label = new QLabel(html);
  label->setTextInteractionFlags(Qt::TextSelectableByMouse);
  auto* scroll = new QScrollArea;
  scroll->setWidget(label);
  scroll->setWidgetResizable(true);
  layout->addWidget(scroll);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(420, 380);
  prepareModal(&dialog);
  dialog.exec();
}

} // namespace

void registerInspectCommands(CommandRegistry& registry, const CommandContext& context,
                             QWidget* window, std::function<Selection()> selection) {
  registry.add(measureCommand());
  registry.add(interferenceCommand(context));
  registry.add(sectionCommand());

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
