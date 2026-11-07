// SPDX-License-Identifier: MIT
// Sheet metal in the main window (mitcad#210): Edit Sheet Rules, Show Flat
// Pattern and Export Flat Pattern, the actions of the SHEET METAL group
// besides its feature commands (commands/SheetCommands.cpp).
#include "MainWindow.hpp"

#include <QDir>
#include <QJsonArray>
#include <QtLogging>

#include "commands/Commands.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Dialogs.hpp"

namespace mitcad {

QJsonObject MainWindow::sheetBody(bool flat) const {
  QJsonArray sheets;
  try {
    sheets = queryArray(QStringLiteral("sheet_bodies"));
  } catch (const std::exception&) {
    return {};
  }
  const auto hasFlat = [](const QJsonObject& sheet) { return sheet.value(QStringLiteral("flat_pattern")).isObject(); };
  // The body of the selection: a body, or one of its faces or edges.
  for (const SelectionItem& item : m_selection) {
    for (const QJsonValue& value : sheets) {
      const QJsonObject sheet = value.toObject();
      if (sheet.value(QStringLiteral("body")).toString() == item.owner &&
          (item.kind == SelectKind::Body || item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
           item.kind == SelectKind::Vertex)) {
        return sheet;
      }
    }
  }
  if (flat) {
    for (const QJsonValue& value : sheets) {
      if (hasFlat(value.toObject())) {
        return value.toObject();
      }
    }
    return {};
  }
  return sheets.size() == 1 ? sheets.first().toObject() : QJsonObject();
}

void MainWindow::registerSheetActions() {
  const auto action = [this](const char* id, const QString& name, const char* icon, std::function<void()> run) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Model;
    def.tab = QStringLiteral("SOLID");
    def.group = QStringLiteral("SHEET METAL");
    def.run = std::move(run);
    def.enabled = [this] { return !sheetBody(false).isEmpty() || !sheetBody(true).isEmpty(); };
    return def;
  };

  CommandDef rules = action("sheet.edit_rules", tr("Edit Sheet Rules"), "sheet-rules", [this] {
    QJsonObject sheet = sheetBody(false);
    if (sheet.isEmpty()) {
      sheet = sheetBody(true);
    }
    const QString uid = sheet.value(QStringLiteral("rules")).toString();
    if (uid.isEmpty()) {
      showHint(tr("Edit Sheet Rules: select a sheet-metal body first."));
      return;
    }
    editFeature(uid);
  });
  rules.tooltip = tr("Edits the selected sheet's thickness, bend radius, K-factor and reliefs");
  rules.keywords = {QStringLiteral("sheet metal"), QStringLiteral("thickness"), QStringLiteral("k-factor")};
  m_registry->add(rules);

  CommandDef show = action("sheet.show_flat", tr("Show Flat Pattern"), "flat-pattern", [this] {
    const QJsonObject sheet = sheetBody(true);
    if (!sheet.value(QStringLiteral("flat_pattern")).isObject()) {
      showHint(tr("Show Flat Pattern: the sheet has no Flat Pattern; make one first."));
      return;
    }
    toggleFlatPattern(sheet.value(QStringLiteral("body")).toString());
  });
  show.tooltip = tr("Shows the selected sheet flat, or folded again");
  show.keywords = {QStringLiteral("sheet metal"), QStringLiteral("folded"), QStringLiteral("flat")};
  m_registry->add(show);

  CommandDef exporting = action("sheet.export_dxf", tr("Export Flat Pattern…"), "export-dxf", [this] {
    QJsonObject sheet = sheetBody(true);
    if (sheet.isEmpty()) {
      sheet = sheetBody(false);
    }
    if (sheet.isEmpty()) {
      showHint(tr("Export Flat Pattern: select a sheet-metal body first."));
      return;
    }
    exportFlatPattern(sheet.value(QStringLiteral("body")).toString());
  });
  // The Make tab's Laser group (mitcad#1049).
  exporting.tab = QStringLiteral("MAKE");
  exporting.group = QStringLiteral("LASER");
  exporting.pinned = true;
  exporting.tooltip = tr("Writes the selected sheet's Flat Pattern to a DXF file");
  exporting.keywords = {QStringLiteral("sheet metal"), QStringLiteral("dxf"), QStringLiteral("laser"),
                        QStringLiteral("drawing")};
  m_registry->add(exporting);
}

void MainWindow::toggleFlatPattern(const QString& body) {
  if (!canChangeModel(true)) {
    return;
  }
  bool shown = false;
  for (const QJsonValue& value : queryArray(QStringLiteral("sheet_bodies"))) {
    const QJsonObject sheet = value.toObject();
    if (sheet.value(QStringLiteral("body")).toString() == body) {
      shown = sheet.value(QStringLiteral("flat_pattern")).toObject().value(QStringLiteral("shown")).toBool();
    }
  }
  if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_flat_pattern")},
                       {QStringLiteral("body"), body},
                       {QStringLiteral("shown"), !shown}})) {
    qInfo().noquote() << QStringLiteral("Flat pattern of %1 %2").arg(bodyName(body), shown ? QStringLiteral("folded")
                                                                                           : QStringLiteral("shown"));
  }
}

void MainWindow::exportFlatPattern(const QString& body) {
  const QString name = bodyName(body);
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(name + QStringLiteral(".dxf"));
  // In the design's length unit unless another is chosen (mitcad#1408).
  QString unit = lengthUnit();
  const QString path = dialogSaveDxf(this, tr("Export Flat Pattern"), suggested, unit);
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("export_flat_pattern_dxf")},
                   {QStringLiteral("body"), body},
                   {QStringLiteral("path"), path},
                   {QStringLiteral("units"), unit}},
                  &result)) {
    sheetWarning(this, tr("Export Flat Pattern"), tr("Could not export:\n%1").arg(m_lastError));
    qInfo().noquote() << QStringLiteral("Export of the flat pattern of %1 refused: %2").arg(name, m_lastError);
    return;
  }
  QStringList layers;
  for (const QJsonValue& value : result.value(QStringLiteral("layers")).toArray()) {
    const QJsonObject layer = value.toObject();
    if (layer.value(QStringLiteral("entities")).toInt() > 0) {
      layers << QStringLiteral("%1 %2").arg(layer.value(QStringLiteral("name")).toString()).arg(layer.value(QStringLiteral("entities")).toInt());
    }
  }
  const QString written = result.value(QStringLiteral("units")).toString(unit);
  qInfo().noquote() << QStringLiteral("Exported %1: dxf flat pattern of %2, %3 entities (%4), in %5")
                           .arg(path, name)
                           .arg(result.value(QStringLiteral("entities")).toInt())
                           .arg(layers.join(QStringLiteral(", ")), written);
  showHint(tr("Exported the flat pattern of %1 to %2 (in %3).").arg(name, QDir::toNativeSeparators(path), written));
}

} // namespace mitcad
