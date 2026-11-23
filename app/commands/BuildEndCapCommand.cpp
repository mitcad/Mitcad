// SPDX-License-Identifier: MIT
// End Cap (mitcad#2243; app/COMMANDS.md, BUILD › MODIFY; commands.md,
// "BUILD: profile assemblies", *End caps*): plates that close the open
// ends of profiles and tubes, each made of its member's section by the
// model (`build_end_cap`): flush or set in from the edge, its corners
// chamfered, outward past the end or inward with the member cut back.
// Picks near the members' ends choose them (the A and B labels as Ends
// shows them; several ends at once, a pick near an end standing for that
// end's face); the plates are previewed from `build_end_caps`, and a
// refusal is said in words with the member it is about painted.
#include "BuildEndCapCommand.hpp"

#include <cmath>
#include <exception>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QtLogging>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../browser/DocumentHost.hpp"
#include "../build/BuildModel.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

using build::BuildPart;

const QString kEnds = QStringLiteral("ends");
const QString kDirection = QStringLiteral("direction");
const QString kThickness = QStringLiteral("thickness");
const QString kInset = QStringLiteral("inset");
const QString kChamfer = QStringLiteral("chamfer");
const QString kOutward = QStringLiteral("outward");
const QString kInward = QStringLiteral("inward");
const cmd::Choices kDirections = {{kOutward, QStringLiteral("Outward: past the end")},
                                  {kInward, QStringLiteral("Inward: the component cut back")}};

// The end of a part an item stands for: its end face's (`<F>:end_a`),
// else the end nearer where it was clicked along the part (end A at z = 0
// of its component).
QString endOfItem(const SelectionItem& item, const BuildPart& part) {
  if (item.name.endsWith(QStringLiteral(":end_b"))) {
    return QStringLiteral("b");
  }
  if (item.name.endsWith(QStringLiteral(":end_a"))) {
    return QStringLiteral("a");
  }
  if (item.at && std::isfinite(part.length)) {
    return (*item.at)[2] > part.length / 2.0 ? QStringLiteral("b") : QStringLiteral("a");
  }
  return QStringLiteral("a");
}

// A pick on a profile kept as the face of the end it was picked near
// (`<F>:end_a` of its derived part): two picks on one side near each end
// are two ends, a pick near an end chosen already takes it away again.
SelectionItem endItem(const SelectionItem& item, const CommandState&, const CommandContext& model) {
  const std::optional<BuildPart> part = build::partOf(item, model);
  if (!part || part->role != QStringLiteral("profile")) {
    return item;
  }
  const QString feature = part->raw.value(QStringLiteral("part")).toString();
  if (feature.isEmpty() || !item.owner.startsWith(feature + QLatin1Char('.'))) {
    return item; // a body of the part's own features, or no body
  }
  SelectionItem end{SelectKind::Face, item.owner,
                    QStringLiteral("%1:end_%2").arg(feature, endOfItem(item, *part)), QStringLiteral("plane"),
                    item.occurrence};
  end.at = item.at;
  return end;
}

// A picked end: the part and the end (`a`, `b`), the item that chose it.
struct PickedEnd {
  BuildPart part;
  QString end;
  SelectionItem item;
};

// The ends the picks choose, each once: a profile's near its pick, an end
// cap's own (the model caps its member's end again).
std::vector<PickedEnd> pickedEnds(const CommandState& state, const CommandContext& model) {
  std::vector<PickedEnd> ends;
  QSet<QString> seen;
  for (const SelectionItem& item : state.items(kEnds)) {
    const std::optional<BuildPart> part = build::partOf(item, model);
    if (!part) {
      continue;
    }
    const QString end = part->role == QStringLiteral("end_cap")
                            ? str(part->raw.value(QStringLiteral("end_cap")).toObject(), "end")
                            : endOfItem(item, *part);
    const QString key = part->component + QLatin1Char('/') + end;
    if (seen.contains(key)) {
      continue;
    }
    seen.insert(key);
    ends.push_back({*part, end, item});
  }
  return ends;
}

QJsonArray endsJson(const std::vector<PickedEnd>& ends) {
  QJsonArray out;
  for (const PickedEnd& end : ends) {
    out.append(QJsonObject{{QStringLiteral("component"), end.part.component}, {QStringLiteral("end"), end.end}});
  }
  return out;
}

// The panel's values as build_end_cap's fields: a zero inset or chamfer
// left out.
QJsonObject capFields(const CommandState& state) {
  QJsonObject fields{{kThickness, state.expression(kThickness)}, {kDirection, state.choice(kDirection)}};
  if (cmd::nonZero(state, kInset)) {
    fields.insert(kInset, state.expression(kInset));
  }
  if (cmd::nonZero(state, kChamfer)) {
    fields.insert(kChamfer, state.expression(kChamfer));
  }
  return fields;
}

// A plate of build_end_caps' preview as a solid: its outline at its start
// swept along its thickness; empty when OCCT cannot make it.
TopoDS_Shape plateShape(const QJsonObject& plate) {
  const QJsonArray outline = plate.value(QStringLiteral("outline")).toArray();
  const std::optional<gp_Pnt> along = pointFrom(plate.value(QStringLiteral("along")));
  if (outline.size() < 3 || !along) {
    return TopoDS_Shape();
  }
  try {
    BRepBuilderAPI_MakePolygon polygon;
    for (const QJsonValue& value : outline) {
      if (const std::optional<gp_Pnt> point = pointFrom(value)) {
        polygon.Add(*point);
      }
    }
    polygon.Close();
    if (!polygon.IsDone()) {
      return TopoDS_Shape();
    }
    BRepBuilderAPI_MakeFace face(polygon.Wire(), true);
    if (!face.IsDone()) {
      return TopoDS_Shape();
    }
    return BRepPrimAPI_MakePrism(face.Face(), gp_Vec(along->X(), along->Y(), along->Z())).Shape();
  } catch (...) {
    return TopoDS_Shape(); // OCCT's Standard_Failure: no preview of it
  }
}

// The A and B labels of every profile's ends (build_end_meets without
// what they meet), the chosen ones marked.
QJsonArray endMarkers(const std::vector<PickedEnd>& chosen, const CommandContext& model) {
  QJsonArray ends;
  try {
    ends = model
               .queryObject({{QStringLiteral("query"), QStringLiteral("build_end_meets")},
                             {QStringLiteral("meets"), false}})
               .value(QStringLiteral("ends"))
               .toArray();
  } catch (const std::exception&) {
    return {};
  }
  QJsonArray markers;
  for (const QJsonValue& value : ends) {
    const QJsonObject end = value.toObject();
    const std::optional<gp_Pnt> point = pointFrom(end.value(QStringLiteral("point")));
    if (!point) {
      continue;
    }
    const QString label = str(end, "end").toUpper();
    bool picked = false;
    for (const PickedEnd& c : chosen) {
      picked = picked || (c.part.component == str(end, "component") && c.end == str(end, "end"));
    }
    markers.append(QJsonObject{{QStringLiteral("label"), picked ? QStringLiteral("> %1").arg(label) : label},
                               {QStringLiteral("point"), build::pointJson(*point)}});
  }
  return markers;
}

std::vector<PreviewMarker> markersOfReport(const QJsonObject& report) {
  std::vector<PreviewMarker> markers;
  for (const QJsonValue& value : report.value(QStringLiteral("markers")).toArray()) {
    const QJsonObject marker = value.toObject();
    if (const std::optional<gp_Pnt> point = pointFrom(marker.value(QStringLiteral("point")))) {
      markers.push_back({str(marker, "label"), *point});
    }
  }
  return markers;
}

// The cut list's lengths of the members and its plates, for the log (UI
// tests): "Post 595 mm x1; 2 x Plate 40 x 40 x 5".
QString listsText(const CommandContext& model) {
  QJsonObject list;
  try {
    list = model.queryObject({{QStringLiteral("query"), QStringLiteral("build_cut_list")}});
  } catch (const std::exception&) {
    return QString();
  }
  QStringList parts;
  for (const QJsonValue& value : list.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject row = value.toObject();
    parts << QStringLiteral("%1 %2 mm x%3")
                 .arg(str(row, "designation"), cmd::number(std::round(row.value(QStringLiteral("length")).toDouble() * 1e3) / 1e3))
                 .arg(row.value(QStringLiteral("quantity")).toInt());
  }
  QStringList plates;
  for (const QJsonValue& value : list.value(QStringLiteral("plates")).toArray()) {
    const QJsonObject plate = value.toObject();
    plates << QStringLiteral("%1 x %2").arg(plate.value(QStringLiteral("quantity")).toInt()).arg(str(plate, "designation"));
  }
  return parts.join(QStringLiteral(", ")) + QStringLiteral("; plates: ") +
         (plates.isEmpty() ? QStringLiteral("none") : plates.join(QStringLiteral(", ")));
}

} // namespace

CommandDef buildEndCapCommand(DocumentHost& host, const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("build.end_cap");
  def.name = QObject::tr("End Cap");
  def.icon = QStringLiteral("build-end-cap");
  def.tooltip = QObject::tr("Closes the open ends of profiles and tubes with plates of their sections");
  def.keywords = {QStringLiteral("cap"),   QStringLiteral("end plate"), QStringLiteral("plate"),
                  QStringLiteral("close"), QStringLiteral("tube"),      QStringLiteral("build")};
  def.inputs = {
      selectionInput(kEnds, QObject::tr("Ends"), SelectKind::Face | SelectKind::Body, 0, 256)
          .withConvertNow(endItem)
          .withTooltip(QObject::tr("Click near each end to cap: the ends show A and B; click an end again to "
                                   "leave it out")),
      choiceInput(kDirection, QObject::tr("Direction"), kDirections, kOutward)
          .withTooltip(QObject::tr("Outward: the plate past the end; Inward: the component cut back, the plate "
                                   "in its place")),
      valueInput(kThickness, QObject::tr("Thickness"), ValueKind::Length, QStringLiteral("5 mm")),
      valueInput(kInset, QObject::tr("Inset"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("The plate set in from the outside of the section; 0: flush")),
      valueInput(kChamfer, QObject::tr("Chamfer"), ValueKind::Length, QStringLiteral("0 mm"))
          .withTooltip(QObject::tr("The plate's corners chamfered, at least as much as their round; 0: none")),
  };
  def.enabled = [&context] { return build::hasBuildParts(context); };
  // The panel stays for the next ends, its values as they are.
  def.repeats = true;
  def.previewMarkers = markersOfReport;
  def.inspect = [](const CommandState& state, const CommandContext& model) {
    Inspection result;
    const std::vector<PickedEnd> ends = pickedEnds(state, model);
    const QJsonArray markers = endMarkers(ends, model);
    result.report = {{QStringLiteral("markers"), markers}};
    if (ends.empty()) {
      result.viewHint = QObject::tr("Click near the ends to cap: A or B; several at once");
      result.error = QObject::tr("Pick the ends of profiles or tubes of the Build tab to cap.");
      result.log = QStringLiteral("waiting for ends; %1 end labels").arg(markers.size());
      return result;
    }
    QJsonObject query = capFields(state);
    query.insert(QStringLiteral("query"), QStringLiteral("build_end_caps"));
    query.insert(kEnds, endsJson(ends));
    QJsonObject answer;
    try {
      answer = model.queryObject(query);
    } catch (const std::exception& error) {
      result.error = errorText(error);
      result.log = QStringLiteral("not previewed: %1").arg(modelText(error));
      return result;
    }
    QStringList lines;
    QStringList logged;
    TopoDS_Compound plates;
    BRep_Builder builder;
    builder.MakeCompound(plates);
    bool any = false;
    for (const QJsonValue& value : answer.value(QStringLiteral("plates")).toArray()) {
      const QJsonObject plate = value.toObject();
      const TopoDS_Shape shape = plateShape(plate);
      if (!shape.IsNull()) {
        builder.Add(plates, shape);
        any = true;
      }
      const QJsonArray size = plate.value(QStringLiteral("size")).toArray();
      const double width = size.at(0).toDouble();
      const double height = size.at(1).toDouble();
      const QString component = str(plate, "component");
      const QString end = str(plate, "end");
      const QString plateText = plate.value(QStringLiteral("disc")).toBool()
                                    ? QObject::tr("a disc of Ø%1 mm").arg(cmd::number(std::round(width * 100.0) / 100.0))
                                    : QObject::tr("a plate of %1 × %2 mm")
                                          .arg(cmd::number(std::round(width * 100.0) / 100.0),
                                               cmd::number(std::round(height * 100.0) / 100.0));
      QString line = QObject::tr("%1, end %2: %3")
                         .arg(cmd::componentName(component, model).toHtmlEscaped(), end.toUpper(), plateText);
      if (!plate.value(QStringLiteral("cap")).isNull()) {
        line += QObject::tr(" (changes its end cap)");
      }
      lines << line;
      logged << QStringLiteral("%1 %2 %3x%4%5")
                    .arg(component, end, cmd::number(std::round(width * 100.0) / 100.0),
                         cmd::number(std::round(height * 100.0) / 100.0),
                         plate.value(QStringLiteral("cap")).isNull() ? QString() : QStringLiteral(" (changed)"));
    }
    if (any) {
      result.shape = plates;
    }
    // Refusals in words, the member each is about painted.
    const QJsonArray problems = answer.value(QStringLiteral("problems")).toArray();
    QStringList refused;
    for (const QJsonValue& value : problems) {
      const QJsonObject problem = value.toObject();
      const QString text = mitcad::modelMessage(str(problem, "problem"));
      refused << text;
      const QString component = str(problem, "component");
      for (const PickedEnd& end : ends) {
        if (!component.isEmpty() && end.part.component == component) {
          result.highlights.append(end.item);
        }
      }
    }
    if (!refused.isEmpty()) {
      result.error = refused.join(QStringLiteral(" "));
    }
    // Slot profiles: their library's caps come first.
    QSet<QString> slotted;
    for (const PickedEnd& end : ends) {
      if (end.part.raw.value(QStringLiteral("slotted")).toBool() && !slotted.contains(end.part.component)) {
        slotted.insert(end.part.component);
        lines << QObject::tr("%1 has slots: a cap of its slot system from the library (Profile, placed on its end) "
                             "fits it first; End Cap makes a plate of its section too.")
                     .arg(end.part.shownName().toHtmlEscaped());
      }
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = QStringLiteral("%1 end(s): %2; %3 %4; %5 problem(s)%6; %7 end labels")
                     .arg(ends.size())
                     .arg(logged.join(QStringLiteral(", ")), state.choice(kDirection), state.expression(kThickness))
                     .arg(problems.size())
                     .arg(refused.isEmpty() ? QString() : QStringLiteral(": ") + refused.join(QStringLiteral(" ")))
                     .arg(markers.size());
    return result;
  };
  def.apply = [&host](CommandState& state, const CommandContext& model) {
    const std::vector<PickedEnd> ends = pickedEnds(state, model);
    if (ends.empty()) {
      host.showStatus(QObject::tr("Pick the ends to cap: click near the ends of profiles or tubes of the Build tab."),
                      NoticeLevel::Error);
      return false;
    }
    QJsonObject command = capFields(state);
    command.insert(QStringLiteral("cmd"), QStringLiteral("build_end_cap"));
    command.insert(kEnds, endsJson(ends));
    QJsonObject answer;
    if (!host.runModelCommand(command, &answer)) {
      return false;
    }
    QStringList made;
    for (const QJsonValue& value : answer.value(QStringLiteral("caps")).toArray()) {
      const QJsonObject cap = value.toObject();
      made << QStringLiteral("%1 on %2 end %3%4")
                  .arg(str(cap, "component"), str(cap, "member"), str(cap, "end"),
                       cap.value(QStringLiteral("new")).toBool() ? QString() : QStringLiteral(" (changed)"));
    }
    qInfo().noquote() << QStringLiteral("End Cap: %1 cap(s) %2 %3: %4")
                             .arg(made.size())
                             .arg(state.choice(kDirection), state.expression(kThickness), made.join(QStringLiteral(", ")));
    qInfo().noquote() << QStringLiteral("End Cap lists: %1").arg(listsText(model));
    // The next ends start afresh.
    state.setItems(kEnds, Selection());
    return true;
  };
  return def;
}

} // namespace mitcad
