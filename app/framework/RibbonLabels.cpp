// SPDX-License-Identifier: MIT
#include "RibbonLabels.hpp"

#include <QChar>
#include <QCoreApplication>
#include <QSettings>

#include <utility>

namespace mitcad {

namespace {

// The toolbar's buttons whose names do not fit under an icon (more than
// ten letters): a word or two that tells them apart on their tab
// (mitcad#1975, proposal P6).
const std::pair<const char*, const char*> kShortLabels[] = {
    // Solid
    {"construct.offset_plane", QT_TRANSLATE_NOOP("RibbonLabel", "Offset")},
    {"construct.plane_angle", QT_TRANSLATE_NOOP("RibbonLabel", "Angle")},
    {"insert.component", QT_TRANSLATE_NOOP("RibbonLabel", "Insert")},
    {"insert.mesh", QT_TRANSLATE_NOOP("RibbonLabel", "Mesh")},
    {"insert.dxf", QT_TRANSLATE_NOOP("RibbonLabel", "DXF")},
    {"file.render_image", QT_TRANSLATE_NOOP("RibbonLabel", "Render")},
    // Surface and Sheet Metal
    {"surface.extrude", QT_TRANSLATE_NOOP("RibbonLabel", "Extrude")},
    {"sheet.flat_pattern", QT_TRANSLATE_NOOP("RibbonLabel", "Flat")},
    {"sheet.rules", QT_TRANSLATE_NOOP("RibbonLabel", "Rules")},
    // Build
    {"build.linear", QT_TRANSLATE_NOOP("RibbonLabel", "Linear")},
    {"build.floor_roof", QT_TRANSLATE_NOOP("RibbonLabel", "Floor/Roof")},
    {"build.spacing", QT_TRANSLATE_NOOP("RibbonLabel", "Spacing")},
    {"build.frame", QT_TRANSLATE_NOOP("RibbonLabel", "Frame")},
    {"build.parts_list", QT_TRANSLATE_NOOP("RibbonLabel", "BOM")},
    // Make
    {"view.annotations", QT_TRANSLATE_NOOP("RibbonLabel", "Notes")},
    {"inspect.print_analysis", QT_TRANSLATE_NOOP("RibbonLabel", "Analysis")},
    {"sheet.export_dxf", QT_TRANSLATE_NOOP("RibbonLabel", "Flat DXF")},
    {"make.export_face_dxf", QT_TRANSLATE_NOOP("RibbonLabel", "Face DXF")},
    // Manage
    {"solid.parameters", QT_TRANSLATE_NOOP("RibbonLabel", "Parameters")},
    {"solid.configurations", QT_TRANSLATE_NOOP("RibbonLabel", "Configs")},
    {"manage.recompute_all", QT_TRANSLATE_NOOP("RibbonLabel", "Recompute")},
    {"manage.plastic_rules", QT_TRANSLATE_NOOP("RibbonLabel", "Plastic")},
    // Sketch
    {"sketch.rectangle", QT_TRANSLATE_NOOP("RibbonLabel", "Rectangle")},
    {"sketch.circle", QT_TRANSLATE_NOOP("RibbonLabel", "Circle")},
    {"sketch.arc", QT_TRANSLATE_NOOP("RibbonLabel", "Arc")},
    {"sketch.dimension", QT_TRANSLATE_NOOP("RibbonLabel", "Dimension")},
    {"sketch.fillet", QT_TRANSLATE_NOOP("RibbonLabel", "Fillet")},
    {"sketch.constraint.horizontal_vertical", QT_TRANSLATE_NOOP("RibbonLabel", "Horiz/Vert")},
    {"sketch.constraint.perpendicular", QT_TRANSLATE_NOOP("RibbonLabel", "Perpend.")},
    {"sketch.insert_dxf", QT_TRANSLATE_NOOP("RibbonLabel", "DXF")},
    {"sketch.finish", QT_TRANSLATE_NOOP("RibbonLabel", "Finish")},
    // Drawing and Cut Sheet
    {"drawing.close", QT_TRANSLATE_NOOP("RibbonLabel", "Close")},
    {"drawing.projected_view", QT_TRANSLATE_NOOP("RibbonLabel", "Projected")},
    {"drawing.section_view", QT_TRANSLATE_NOOP("RibbonLabel", "Section")},
    {"drawing.detail_view", QT_TRANSLATE_NOOP("RibbonLabel", "Detail")},
    {"drawing.center_mark", QT_TRANSLATE_NOOP("RibbonLabel", "Center")},
    {"drawing.center_pattern", QT_TRANSLATE_NOOP("RibbonLabel", "Pattern")},
    {"drawing.edge_extension", QT_TRANSLATE_NOOP("RibbonLabel", "Extension")},
    {"drawing.model_items", QT_TRANSLATE_NOOP("RibbonLabel", "Model")},
    {"drawing.datum", QT_TRANSLATE_NOOP("RibbonLabel", "Datum")},
    {"drawing.frame", QT_TRANSLATE_NOOP("RibbonLabel", "Frame")},
    {"drawing.finish", QT_TRANSLATE_NOOP("RibbonLabel", "Finish")},
    {"drawing.parts_list", QT_TRANSLATE_NOOP("RibbonLabel", "BOM")},
    {"drawing.add_parts", QT_TRANSLATE_NOOP("RibbonLabel", "Add")},
};

} // namespace

QString defaultShortLabel(const QString& id) {
  const QByteArray wanted = id.toUtf8();
  for (const auto& [command, label] : kShortLabels) {
    if (wanted == command) {
      return QCoreApplication::translate("RibbonLabel", label);
    }
  }
  return QString();
}

QStringList shortLabelIds() {
  QStringList ids;
  for (const auto& entry : kShortLabels) {
    ids.append(QString::fromLatin1(entry.first));
  }
  return ids;
}

QString ribbonLabelsId(RibbonLabels labels) {
  return labels == RibbonLabels::UnderIcons ? QStringLiteral("labels") : QStringLiteral("icons");
}

std::optional<RibbonLabels> ribbonLabelsFromId(const QString& id) {
  if (id == QLatin1String("icons")) {
    return RibbonLabels::IconsOnly;
  }
  if (id == QLatin1String("labels")) {
    return RibbonLabels::UnderIcons;
  }
  return std::nullopt;
}

QString ribbonLabelsKey() { return QStringLiteral("appearance/ribbonLabels"); }

namespace {
RibbonLabels g_defaultLabels = RibbonLabels::IconsOnly;
} // namespace

RibbonLabels defaultRibbonLabels() { return g_defaultLabels; }

void setDefaultRibbonLabels(RibbonLabels labels) { g_defaultLabels = labels; }

RibbonLabels savedRibbonLabels() {
  return ribbonLabelsFromId(QSettings().value(ribbonLabelsKey()).toString()).value_or(g_defaultLabels);
}

bool saveRibbonLabels(RibbonLabels labels) {
  if (savedRibbonLabels() == labels) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(ribbonLabelsKey(), ribbonLabelsId(labels));
  }
  emit RibbonLabelsNotifier::instance().labelsChanged(labels == RibbonLabels::UnderIcons);
  return true;
}

RibbonLabelsNotifier& RibbonLabelsNotifier::instance() {
  static RibbonLabelsNotifier notifier;
  return notifier;
}

QString ribbonButtonLabel(const QString& name, const QString& shortLabel) {
  QString label = shortLabel.isEmpty() ? name : shortLabel;
  // The mnemonic's "&" goes; "&&" is a literal ampersand.
  QString plain;
  plain.reserve(label.size());
  for (int i = 0; i < label.size(); ++i) {
    if (label.at(i) == QLatin1Char('&') && i + 1 < label.size()) {
      ++i;
    }
    plain.append(label.at(i));
  }
  label = plain.simplified();
  if (label.endsWith(QChar(0x2026))) {
    label.chop(1);
  } else if (label.endsWith(QLatin1String("..."))) {
    label.chop(3);
  }
  return label.trimmed();
}

double ribbonLabelFontScale(int iconPercent, int textPercent) {
  if (iconPercent <= 0 || textPercent <= 0) {
    return 0.85;
  }
  return 0.85 * iconPercent / textPercent;
}

int ribbonLabelWidthLimit(int iconPixels) { return iconPixels * 5 / 2; }

} // namespace mitcad
