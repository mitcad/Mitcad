// SPDX-License-Identifier: MIT
#include "CurvatureCombCommand.hpp"

#include <cmath>
#include <exception>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QStringList>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "../framework/Command.hpp"
#include "../framework/LengthUnits.hpp"
#include "CurvatureComb.hpp"

namespace mitcad {

namespace {

gp_Pnt toPnt(const CombPoint& p) { return gp_Pnt(p[0], p[1], p[2]); }

// A pick as the query takes it, with the occurrence that places it, so that
// the comb is drawn where the view shows the curve.
QJsonValue combItem(const SelectionItem& item) {
  const QJsonValue reference = item.reference();
  if (!reference.isObject()) {
    return reference;
  }
  QJsonObject ref = reference.toObject();
  if (!item.occurrence.isEmpty()) {
    ref.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  return ref;
}

// The panel's Teeth and Size, when they are within their ranges.
bool settingsOf(const CommandState& state, CurvatureCombSettings& settings) {
  const double teeth = state.value(QStringLiteral("teeth"));
  const double size = state.value(QStringLiteral("size"));
  if (!std::isfinite(teeth) || std::abs(teeth - std::round(teeth)) > 1e-9) {
    return false;
  }
  settings.teeth = static_cast<int>(std::lround(teeth));
  settings.size = size;
  return settings.valid();
}

Inspection::Value row(const QString& name, const QString& value) {
  Inspection::Value v;
  v.name = name;
  v.value = value;
  return v;
}

} // namespace

CommandDef curvatureCombCommand() {
  CommandDef def;
  def.id = QStringLiteral("inspect.curvature_comb");
  def.name = QObject::tr("Curvature Comb");
  def.icon = QStringLiteral("curvature-comb");
  def.tooltip = QObject::tr("How sharply sketch curves and edges bend, as a comb, with inflection points");
  def.keywords = {QStringLiteral("curvature"), QStringLiteral("comb"), QStringLiteral("inflection"),
                  QStringLiteral("porcupine"), QStringLiteral("smoothness"), QStringLiteral("radius")};
  // It only shows: Close (as Measure). Sketch mode has its own Curvature
  // comb option for the sketch it edits.
  def.closeOnly = true;
  def.inputs = {
      selectionInput(QStringLiteral("curves"), QObject::tr("Curves"), SelectKind::Edge | SelectKind::SketchCurve, 1,
                     0)
          .withSketchesBehind()
          .withTooltip(QObject::tr("Sketch curves and edges to comb: a click adds or removes one")),
      valueInput(QStringLiteral("teeth"), QObject::tr("Density"), ValueKind::Unitless,
                 QString::number(CurvatureCombSettings::kDefaultTeeth))
          .withTooltip(QObject::tr("Teeth along each curve, 2 to 1000")),
      valueInput(QStringLiteral("size"), QObject::tr("Scale"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The longest tooth's length: 1 is a quarter of the curves' size")),
  };
  // The Density and Scale used last, here and in sketch mode.
  def.init = [](CommandState& state, const CommandContext&) {
    const CurvatureCombSettings kept = CurvatureCombSettings::load();
    state.setText(QStringLiteral("teeth"), QString::number(kept.teeth));
    state.setText(QStringLiteral("size"), QString::number(kept.size, 'g', 6));
  };
  def.inspect = [](const CommandState& state, const CommandContext& context) {
    Inspection result;
    CurvatureCombSettings settings;
    if (!settingsOf(state, settings)) {
      result.error = QObject::tr("Density: a whole number from 2 to 1000; Scale: more than 0.");
      return result;
    }
    settings.save();
    QJsonArray items;
    for (const SelectionItem& item : state.items(QStringLiteral("curves"))) {
      items.append(combItem(item));
    }
    if (items.isEmpty()) {
      result.error = QObject::tr("Select sketch curves or edges.");
      return result;
    }
    QJsonObject answer;
    try {
      answer = context.queryObject(curvatureCombQuery(items, settings));
    } catch (const std::exception& e) {
      result.error = QString::fromUtf8(e.what());
      return result;
    }
    const CurvatureCombDrawing drawing = curvatureCombDrawing(answer);
    // The teeth and the envelope over the view.
    TopoDS_Compound drawn;
    BRep_Builder builder;
    builder.MakeCompound(drawn);
    bool any = false;
    const auto add = [&](const CombPoint& a, const CombPoint& b) {
      const gp_Pnt p = toPnt(a);
      const gp_Pnt q = toPnt(b);
      if (p.Distance(q) > 1e-9) {
        builder.Add(drawn, BRepBuilderAPI_MakeEdge(p, q).Edge());
        any = true;
      }
    };
    for (const auto& [a, b] : drawing.teethLines()) {
      add(a, b);
    }
    for (const auto& [a, b] : drawing.envelopeLines()) {
      add(a, b);
    }
    if (any) {
      result.shape = drawn;
    }
    // The inflection points as labelled markers (previewMarkers).
    QJsonArray inflections;
    for (const CombPoint& p : drawing.inflections) {
      inflections.append(QJsonArray{p[0], p[1], p[2]});
    }
    result.report = QJsonObject{{QStringLiteral("inflections"), inflections}};
    const QString unit = context.lengthUnit();
    result.values.push_back(row(QObject::tr("Curves"), QString::number(drawing.answered)));
    Inspection::Value turns = row(QObject::tr("Inflections"), QString::number(drawing.inflections.size()));
    turns.main = true;
    result.values.push_back(turns);
    if (drawing.minRadius > 0.0) {
      Inspection::Value radius = row(QObject::tr("Smallest radius"), lengthInUnit(drawing.minRadius, unit));
      radius.tip = QObject::tr("The tightest bend of the picked curves: the longest tooth");
      result.values.push_back(radius);
    }
    QStringList lines;
    lines << QObject::tr("<b>Inflections: %1</b>").arg(drawing.inflections.size());
    lines << QObject::tr("Curves: %1 (%2 straight)").arg(drawing.answered).arg(drawing.straight);
    if (drawing.minRadius > 0.0) {
      lines << QObject::tr("Smallest radius: %1").arg(lengthInUnit(drawing.minRadius, unit));
    }
    for (const QString& error : drawing.errors) {
      lines << error;
    }
    result.text = lines.join(QStringLiteral("<br>"));
    result.log = curvatureCombLog(drawing);
    if (drawing.answered == 0 && !drawing.errors.isEmpty()) {
      result.error = drawing.errors.join(QStringLiteral("; "));
    }
    return result;
  };
  def.previewMarkers = [](const QJsonObject& report) {
    std::vector<PreviewMarker> markers;
    for (const QJsonValue& value : report.value(QStringLiteral("inflections")).toArray()) {
      const QJsonArray p = value.toArray();
      PreviewMarker marker;
      marker.label = QObject::tr("Inflection");
      marker.point = gp_Pnt(p.at(0).toDouble(), p.at(1).toDouble(), p.at(2).toDouble());
      markers.push_back(marker);
    }
    return markers;
  };
  return def;
}

} // namespace mitcad
