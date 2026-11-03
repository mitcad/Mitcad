// SPDX-License-Identifier: MIT
// Change Parameters while a sketch or a feature is being created or edited
// (mitcad#564): SKETCH > MODIFY's entry, the SOLID entry and the command
// search while a panel is open, a button in the panel's title row, and the
// parameter table's commands run as the window's mode needs them.
//
// In sketch mode a change is a model command and an undo step as any
// sketch action (Undo in the sketch takes it back; the edit of an existing
// sketch makes it part of its one undo step at Finish Sketch), and the
// sketch is read again, so its dimensions and geometry follow. While a
// panel is open, its session runs it outside the command's undo range
// (CommandSessionParameters.cpp): Cancel keeps it, OK adds or edits the
// feature after it; the names and expressions of an edited feature's own
// parameters stay the panel's.
#include "MainWindow.hpp"

#include <exception>

#include <QAction>
#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>
#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "browser/DocumentSnapshot.hpp"
#include "browser/ParametersDialog.hpp"
#include "framework/Command.hpp"
#include "framework/CommandPanel.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
#include "framework/ParameterEdits.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeForms.hpp"
#endif
#include "sketch/SketchController.hpp"
#include "sketch/SketchModel.hpp"

namespace mitcad {
namespace {

// The dialog's property that says its close is followed (once per dialog).
const char* const kFollowed = "mitcadParameterTableFollowed";

// Whether a panel has a value field, also in a list's rows: where its
// expressions can use the parameters (the definition's inputs, a list's
// children).
template <typename Inputs>
bool hasValueInput(const Inputs& inputs) {
  for (const InputDef& input : inputs) {
    if (input.type == InputDef::Type::Value ||
        (input.type == InputDef::Type::List && hasValueInput(input.children))) {
      return true;
    }
  }
  return false;
}

} // namespace

void MainWindow::registerParameterEditCommands() {
  CommandDef def;
  def.id = QStringLiteral("sketch.parameters");
  def.name = tr("Edit Parameters…");
  def.icon = QStringLiteral("parameters");
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Sketch;
  def.tab = QStringLiteral("SKETCH");
  def.group = QStringLiteral("MODIFY");
  def.tooltip = tr("User and model parameters; the sketch stays open and follows them");
  def.keywords = {QStringLiteral("parameters"), QStringLiteral("user parameter"), QStringLiteral("expression"),
                  QStringLiteral("fx")};
  def.enabled = [this] { return !windowReadOnly(); };
  def.run = [this] { openParameterTable(); };
  m_registry->add(def);
}

void MainWindow::openParameterTable() {
  m_controller->openParameters();
  ParametersDialog* dialog = m_controller->parametersDialog();
  if (dialog != nullptr && !dialog->property(kFollowed).toBool()) {
    dialog->setProperty(kFollowed, true);
    connect(dialog, &QDialog::finished, this, [this] { parameterTableClosed(); });
  }
}

void MainWindow::parameterTableClosed() {
  const ParameterEditScope scope = parameterScope();
  if (scope.isEmpty()) {
    return;
  }
  // Back to what stayed open: the panel or the sketch takes the keys again.
  if (!m_nativeUi) {
    window()->raise(); // the document frame's window too (mitcad#1172)
    activateWindow();
  }
  qDebug().noquote() << QStringLiteral("Parameters closed: back to %1")
                            .arg(scope.command.isEmpty() ? scope.sketch : scope.command);
}

void MainWindow::addParametersButton(CommandPanel& panel, const CommandDef& def) {
  if (hasValueInput(def.inputs)) {
    panel.addHeaderAction(m_registry->action(QStringLiteral("solid.parameters")));
  }
}

#ifdef MITCAD_MACOS_NATIVE_UI
// The AppKit command form's title row has the same button (mitcad#815).
void MainWindow::addParametersButton(NativeCommandForm& form, const CommandDef& def) {
  if (hasValueInput(def.inputs)) {
    form.addHeaderAction(m_registry->action(QStringLiteral("solid.parameters")));
  }
}
#endif

ParameterEditScope MainWindow::parameterScope() const {
  ParameterEditScope scope;
  if (m_sketch->isActive() && (m_mode == Mode::Sketch || (m_mode == Mode::Command && m_commandInSketch))) {
    scope.sketch = m_sketch->model().name;
  }
  if (m_mode == Mode::Command && m_session) {
    scope.command = m_session->def().name;
    if (m_session->isEditing()) {
      scope.editedFeature = m_session->editUid();
      const DocumentSnapshot::Feature* feature = m_snapshot.feature(scope.editedFeature);
      scope.editedName = feature != nullptr ? feature->name : QString();
    }
  }
  return scope;
}

bool MainWindow::runParameterCommand(const QJsonObject& command) {
  const ParameterEditScope scope = parameterScope();
  const QString name = command.value(QStringLiteral("name")).toString();
  if (!scope.editedFeature.isEmpty() && !name.isEmpty()) {
    // The edited feature's own name and expression are its panel's.
    QString owner;
    try {
      for (const QJsonValue& value : queryArray(QStringLiteral("parameters"))) {
        const QJsonObject parameter = value.toObject();
        if (parameter.value(QStringLiteral("name")).toString() == name) {
          owner = parameter.value(QStringLiteral("owner")).toString();
        }
      }
    } catch (const std::exception&) {
      // Not known: the model answers the command itself.
    }
    QString reason;
    if (scope.refuses(command, owner, &reason)) {
      m_lastError = reason;
      m_lastCancelled = false;
      showError(reason);
      qInfo().noquote() << QStringLiteral("Parameter change refused during %1: %2").arg(scope.describe(), reason);
      return false;
    }
  }
  if (m_mode == Mode::Command && m_session) {
    QString refusal;
    if (m_session->runParameter(command, refusal)) {
      return true;
    }
    if (!refusal.isEmpty()) {
      m_lastError = refusal;
      m_lastCancelled = false;
    }
    return false;
  }
  const bool done = runModelCommand(command);
  if (done && m_mode == Mode::Sketch && m_sketch->isActive() && parameterCommandChangesValues(command)) {
    logSketchDimensions(QStringLiteral("after the parameter change"));
  }
  return done;
}

void MainWindow::logSketchDimensions(const QString& why) {
  QStringList values;
  for (const sketch::DimensionData& dimension : m_sketch->model().dimensions) {
    if (!dimension.parameter.isEmpty()) {
      values << QStringLiteral("%1 %2").arg(dimension.parameter, QString::number(dimension.measured, 'f', 3));
    }
  }
  qDebug().noquote() << QStringLiteral("Sketch %1 dimensions %2: %3")
                            .arg(m_sketch->model().name, why,
                                 values.isEmpty() ? QStringLiteral("none") : values.join(QStringLiteral(", ")));
}

} // namespace mitcad
