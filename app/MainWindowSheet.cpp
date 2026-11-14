// SPDX-License-Identifier: MIT
// Sheet metal in the main window (mitcad#210): Edit Sheet Rules and Show
// Flat Pattern, the actions of the SHEET METAL group besides its feature
// commands (commands/SheetCommands.cpp, which registers them in their
// sections), and Export Flat Pattern (the Make tab).
#include "MainWindow.hpp"

#include <QDir>
#include <QJsonArray>
#include <QStackedWidget>
#include <QTimer>
#include <QtLogging>

#include <utility>

#include "commands/Commands.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
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
  // The group's features with these two in their sections (mitcad#1778):
  // Edit Sheet Rules after Sheet Rules, Show Flat Pattern after Flat Pattern.
  registerSheetMetalCommands(*m_registry, *this, rules, show);

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

// ---------------------------------------------------------------------------
// A sketch edited without leaving the command that uses it (mitcad#1648:
// Flange's Edit Profile). The model command makes the sketch, or moves it
// back onto the wall, before the command's feature; the command's input
// names it; the panel is set aside (its preview gone, nothing previewed)
// while the sketch is open as Edit Sketch opens it, and Finish Sketch (or
// whatever else leaves the sketch, an undo that takes it back among them)
// brings the panel back with its preview computed anew. Cancel then takes
// the sketch back with the command (CommandSession::featuresInserted).

bool MainWindow::editSketchInCommand(const QString& commandId, const QString& input, const QJsonObject& command) {
  if (m_mode != Mode::Command || !m_session || m_session->def().id != commandId || m_session->suspended() ||
      m_sketchingSession) {
    return false;
  }
  const int depth = undoDepth();
  QJsonObject result;
  if (!runCommand(command, &result)) {
    return false; // the model's reason is shown
  }
  const QString sketch = result.value(QStringLiteral("sketch")).toString();
  const int added = result.value(QStringLiteral("added")).toInt();
  if (sketch.isEmpty()) {
    return false;
  }
  m_session->featuresInserted(added, depth);
  m_session->choose(input, sketch);
  const QString name = m_session->def().name;
  qDebug().noquote() << QStringLiteral("%1: editing sketch %2 (%3) in the command, %4 feature(s) added")
                            .arg(name, sketch, featureName(sketch))
                            .arg(added);
  // Once the panel's own action is over.
  const QPointer<CommandSession> session = m_session;
  QTimer::singleShot(0, this, [this, session, input, sketch, added, name] {
    if (!session || session.data() != m_session.data() || m_mode != Mode::Command) {
      qInfo().noquote() << QStringLiteral("%1: the sketch was not opened, the command ended").arg(name);
      return;
    }
    session->suspend();
    m_sketchingSession = session;
    m_sketchingPage = m_panelScroll;
    m_sketchingInput = input;
    m_sketchingSketch = sketch;
    m_sketchingAdded = added;
    m_session = nullptr;
    m_mode = Mode::Idle;
    editSketch(sketch);
    if (m_mode != Mode::Sketch) {
      // The sketch did not open: the panel comes back at once.
      resumeSketchingCommand();
    }
  });
  return true;
}

void MainWindow::resumeSketchingCommand() {
  const QPointer<CommandSession> session = std::exchange(m_sketchingSession, nullptr);
  const QPointer<QWidget> page = std::exchange(m_sketchingPage, nullptr);
  if (!session) {
    return;
  }
  const QString name = session->def().name;
  if (m_mode != Mode::Idle || m_session) {
    // Another command or mode took over from the sketch: the command set
    // aside ends as Cancel ends it, without touching the one running now.
    QObject::disconnect(session.data(), nullptr, this, nullptr);
    session->cancel();
    session->deleteLater();
    if (page) {
      page->deleteLater();
    }
    qInfo().noquote() << QStringLiteral("%1 set aside was cancelled: another command took over").arg(name);
    return;
  }
  if (!sketchExists(m_sketchingSketch)) {
    // An undo in the sketch took its making back: the command no longer
    // names it, nor counts the features made for it.
    session->featuresInserted(-m_sketchingAdded, -1);
    session->choose(m_sketchingInput, QString());
    qDebug().noquote() << QStringLiteral("%1: the sketch %2 is gone").arg(name, m_sketchingSketch);
  }
  m_session = session;
  m_mode = Mode::Command;
  if (page) {
    m_commandPages->setCurrentWidget(page);
  }
  session->resume();
  updateActions();
  qDebug().noquote() << QStringLiteral("%1 back from the sketch %2").arg(name, featureName(m_sketchingSketch));
}

} // namespace mitcad
