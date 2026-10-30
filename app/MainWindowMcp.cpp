// SPDX-License-Identifier: MIT
// Live MCP operates on the window's document, through its ordinary command
// workflow. It never owns another model or accesses the worker's document.
// The application's own state and actions (ui_state, ui_select, ui_command,
// ...) are in MainWindowMcpUi.cpp (mitcad#265).
#include "MainWindow.hpp"
#include "framework/CommandRegistry.hpp"

#include <stdexcept>
#include <utility>

#include <QApplication>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QList>
#include <QScopedValueRollback>
#include <QString>
#include <QStringList>
#include <QThread>

#include "browser/BrowserController.hpp"
#include "files/LockController.hpp"
#include "framework/Json.hpp"
#include "mcp/AgentFolders.hpp"
#include "mcp/AgentIndicator.hpp"
#include "mcp/AssistantSetup.hpp"
#include "mcp/EndpointLimits.hpp"
#include "mcp/LiveErrors.hpp"
#include "mcp/ModalGuard.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeDialogs.hpp"
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
namespace {

// What an agent's change says in the undo text and the history (mitcad#265).
const QString kAgentSuffix = QStringLiteral(" (agent)");

[[noreturn]] void refuse(const QString& reason) {
  // A refusal can repeat a client's text: its log line keeps the start
  // (mitcad#935).
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw std::runtime_error(reason.toStdString());
}

// A refusal with fields of its own beside the error (mitcad#689: which
// entry of a command array).
[[noreturn]] void refuseWith(const QString& reason, const QJsonObject& fields) {
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw mcp::Refusal(reason, fields);
}

} // namespace

void MainWindow::mcpRefuseBusy() const {
  // Only computation refuses everything, queries too (mitcad#265): the
  // agent is told when to retry.
  int retry = 0;
  QString reason;
  if (m_mcpRequestActive) {
    reason = tr("Another external request is running");
    retry = 500;
  } else if (modelBusy()) {
    reason = tr("The design is computing");
    retry = 1000;
  } else if (m_viewer->isPainting()) {
    reason = tr("The view is drawing");
    retry = 100;
  } else if (!m_import.isNull()) {
    reason = tr("An import is running");
    retry = 2000;
  }
  if (retry > 0) {
    const QString message = tr("busy: %1; retry in %2 ms").arg(reason).arg(retry);
    qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(message);
    throw mcp::Refusal(message, {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), retry}});
  }
}

QString MainWindow::mcpBlockedReason(bool editing) const {
  if (m_mcpRequestActive) {
    return tr("Another external design request is running.");
  }
  if (modelBusy() || m_viewer->isPainting()) {
    return tr("The design is busy; retry when computation or drawing has finished.");
  }
  // A sketch being edited takes its own commands (mcpSketchCommand); a
  // command panel or a plane pick takes none (mitcad#265).
  if (m_mode == Mode::Command || m_session || m_panelScroll) {
    const QString name = m_session ? m_session->def().name : tr("a command");
    return tr("The command panel of %1 is open: finish it (ui_command with finish ok or cancel) before changing the "
              "design; queries and the ui tools work meanwhile.")
        .arg(name);
  }
  if (m_mode == Mode::PickPlane) {
    return tr("Create Sketch waits for a plane to be picked: pick one or cancel it first.");
  }
  if (m_drag || m_animation || (m_controller && m_controller->markerDragActive())) {
    return tr("Finish dragging or animating the design before accessing it externally.");
  }
  if (!m_import.isNull()) {
    return tr("An import is running; retry when it has finished.");
  }
  if (m_closePending || QApplication::activeModalWidget() != nullptr) {
    return tr("The design is closing or waiting for a dialog.");
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeForm || m_nativeClosingCommand || nativeDialogModalActive() ||
      (m_nativeWindow && m_nativeWindow->modalBlocked())) {
    return tr("The design is waiting for a native command or dialog.");
  }
#endif
  if (editing && windowReadOnly()) {
    return readOnlyReason();
  }
  return QString();
}

QJsonObject MainWindow::mcpDocumentInfo() const {
  Q_ASSERT(QThread::currentThread() == thread());
  const DocumentSession::Snapshot snapshot = m_documentSession->snapshot();
  return {{QStringLiteral("document"), m_mcpDocumentId},
          {QStringLiteral("path"), snapshot.filePath.isEmpty() ? QJsonValue(QJsonValue::Null)
                                                              : QJsonValue(snapshot.filePath)},
          {QStringLiteral("name"), snapshot.name},
          {QStringLiteral("state"), snapshot.document},
          {QStringLiteral("busy"), snapshot.busy},
          {QStringLiteral("editable"), mcpBlockedReason(true).isEmpty()}};
}

QJsonObject MainWindow::mcpQuery(const QJsonObject& request) const {
  Q_ASSERT(QThread::currentThread() == thread());
  // Read-only: also with a sketch or command open, during drags and
  // dialogs; refused only while the model computes (mitcad#265).
  mcpRefuseBusy();
  // The shared MCP validator excludes disk reads and external workflows.
  validate_mcp_query(rustStr(compactJson(request)));
  return {{QStringLiteral("document"), m_mcpDocumentId}, {QStringLiteral("result"), query(request)}};
}

QJsonObject MainWindow::mcpCommand(const QJsonObject& arguments) {
  Q_ASSERT(QThread::currentThread() == thread());
  mcpRefuseBusy();
  const QString refusal = mcpBlockedReason(true);
  if (!refusal.isEmpty()) {
    refuse(refusal);
  }
  // One command, or an array run in order as one undo step (mitcad#689).
  const QJsonValue given = arguments.value(QStringLiteral("command"));
  const bool array = given.isArray();
  QList<QJsonObject> entries;
  if (array) {
    for (const QJsonValue& entry : given.toArray()) {
      entries.append(entry.toObject());
    }
  } else {
    entries.append(given.toObject());
  }
  if (entries.isEmpty()) {
    refuse(tr("The command array is empty."));
  }
  const auto entryName = [array](qsizetype i) {
    return array ? QStringLiteral("commands[%1]: ").arg(i) : QString();
  };
  const auto isStep = [](const QJsonObject& entry) {
    const QString kind = entry.value(QStringLiteral("cmd")).toString();
    return kind == QStringLiteral("undo") || kind == QStringLiteral("redo");
  };
  for (qsizetype i = 0; i < entries.size(); ++i) {
    if (entries.size() > 1 && isStep(entries[i])) {
      refuse(tr("%1undo and redo run alone, not in a command array.").arg(entryName(i)));
    }
    // By class: file IO, remote and internal checkpoint commands stay
    // unavailable (mitcad#265, core/model-api/src/classes.rs).
    try {
      idleDocument().validate_mcp_document_command(rustStr(compactJson(entries[i])));
    } catch (const std::exception& error) {
      if (!array) {
        refuse(errorText(error));
      }
      // Nothing of the array has run.
      refuseWith(entryName(i) + errorText(error),
                 {{QStringLiteral("entry"), static_cast<qint64>(i)},
                  {QStringLiteral("applied"), false},
                  {QStringLiteral("undone"), 0}});
    }
  }
  const QString mode = arguments.value(QStringLiteral("timeline")).toString(QStringLiteral("changed"));
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  // What the result compares (mitcad#689): the document before, when only
  // the changes are answered.
  const QByteArray before = mode == QStringLiteral("changed") ? compactJson(mcpSnapshot()) : QByteArray();
  const bool inSketch = m_mode == Mode::Sketch;
  const QString sketchUid = inSketch ? m_sketch->uid() : QString();
  const int depth = undoDepth();
  QJsonArray results;
  QJsonArray times;
  qsizetype stopped = -1;
  for (qsizetype i = 0; i < entries.size(); ++i) {
    QJsonObject answer;
    QString reason;
    const bool ran = inSketch ? mcpSketchStep(entries[i], &answer, reason) : mcpModelStep(entries[i], &answer, reason);
    if (!ran) {
      if (!array) {
        refuse(reason);
      }
      // Nothing of the array stays: the entries before it are undone.
      const int undone = mcpUndoTo(depth);
      refuseWith(tr("%1%2; the %3 step(s) of the entries before it were undone").arg(entryName(i), reason).arg(undone),
                 {{QStringLiteral("entry"), static_cast<qint64>(i)},
                  {QStringLiteral("applied"), false},
                  {QStringLiteral("undone"), undone}});
    }
    const QByteArray answerText = compactJson(answer);
    if (mcp_recomputed(rustStr(answerText))) {
      times.append(query({{QStringLiteral("query"), QStringLiteral("recompute_times")}}));
    }
    results.append(answer);
    // An entry whose added or edited feature fails stops the array.
    if (array && i + 1 < entries.size() && mcp_entry_failed(rustStr(answerText))) {
      stopped = i;
      break;
    }
  }
  if (!(entries.size() == 1 && isStep(entries.first()))) {
    if (inSketch) {
      m_mcpSketchEdited = true;
    }
    // The agent's steps as one, labelled (agent).
    mcpLabelAgentChange(depth);
  }
  const QByteArray timesText = QJsonDocument(times).toJson(QJsonDocument::Compact);
  QJsonObject result = parseObject(mcp_command_document(rustStr(mode.toUtf8()), rustStr(before),
                                                        rustStr(compactJson(mcpSnapshot())), rustStr(timesText)));
  result.insert(QStringLiteral("document"), m_mcpDocumentId);
  result.insert(QStringLiteral("applied"), true);
  result.insert(QStringLiteral("result"), results.last());
  if (inSketch) {
    result.insert(QStringLiteral("sketch"), sketchUid);
  }
  if (array) {
    result.insert(QStringLiteral("ran"), static_cast<qint64>(results.size()));
    result.insert(QStringLiteral("results"), results);
    if (stopped >= 0) {
      result.insert(QStringLiteral("stopped_at"), static_cast<qint64>(stopped));
    }
  }
  QStringList kinds;
  for (const QJsonObject& entry : std::as_const(entries)) {
    kinds << entry.value(QStringLiteral("cmd")).toString();
  }
  qInfo().noquote() << (inSketch ? QStringLiteral("MCP: an agent ran %1 in sketch %2").arg(kinds.join(QStringLiteral(", ")), sketchUid)
                                 : QStringLiteral("MCP: an agent ran %1").arg(kinds.join(QStringLiteral(", "))));
  return result;
}

bool MainWindow::mcpModelStep(const QJsonObject& request, QJsonObject* answer, QString& reason) {
  if (!runModelCommand(request, answer)) {
    reason = lastError().isEmpty() ? tr("The model command was refused.") : lastError();
    return false;
  }
  return true;
}

bool MainWindow::mcpSketchStep(const QJsonObject& request, QJsonObject* answer, QString& reason) {
  // The sketch being edited: its own commands go through the sketch
  // session, as the user's would (mitcad#265): one undo step, the view,
  // the palette and the sketch's undo follow.
  const QString kind = request.value(QStringLiteral("cmd")).toString();
  const QString uid = m_sketch->uid();
  const QString sketchName = m_sketch->model().name;
  if (kind == QStringLiteral("undo") || kind == QStringLiteral("redo")) {
    const QString label =
        queryObject({{QStringLiteral("query"), QStringLiteral("document")}}).value(kind).toString();
    if (label.isEmpty()) {
      reason = tr("Nothing to %1.").arg(kind);
      return false;
    }
    if (kind == QStringLiteral("undo")) {
      undo();
    } else {
      redo();
    }
    *answer = {{QStringLiteral("label"), label}};
    return true;
  }
  const bool own =
      (kind.startsWith(QStringLiteral("sketch.")) && request.value(QStringLiteral("sketch")).toString() == uid) ||
      (kind == QStringLiteral("edit_feature") && request.value(QStringLiteral("uid")).toString() == uid);
  if (!own) {
    reason = tr("%1 (%2) is being edited: only its own sketch commands (with \"sketch\": \"%2\"), edit_feature of "
                "it, undo and redo run now. Finish it (ui_command with id sketch.finish) for other commands.")
                 .arg(sketchName, uid);
    return false;
  }
  sketch::SketchOp op(*m_sketch);
  if (!op.run(request, answer)) {
    const QString error = op.error();
    op.rollback();
    reason = error.isEmpty() ? tr("The sketch command was refused.") : error;
    return false;
  }
  op.commit();
  return true;
}

int MainWindow::mcpUndoTo(int depth) {
  // An array's entries taken back after a later one was refused
  // (mitcad#689), as Undo would, in a sketch too.
  int undone = 0;
  for (int now = undoDepth(); now > depth; now = undoDepth()) {
    undo();
    if (undoDepth() >= now) {
      break; // refused
    }
    ++undone;
  }
  return undone;
}

QJsonObject MainWindow::mcpSnapshot() const {
  // The queries a command's result compares (mitcad#689, mcp_snapshot_queries).
  QJsonObject snapshot;
  const QJsonObject queries = parseObject(mcp_snapshot_queries());
  for (auto it = queries.constBegin(); it != queries.constEnd(); ++it) {
    snapshot.insert(it.key(), query(it.value().toObject()));
  }
  return snapshot;
}

QJsonObject MainWindow::mcpExport(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // Into a folder for designs, a new file, as document_save's
  // path (mitcad#689); the document does not change.
  if (m_session || m_mode == Mode::Command) {
    refuse(tr("Finish the current command before exporting."));
  }
  if (autosaveMarker() >= 0) {
    // Editing a sketch in the middle of the timeline rolls the later
    // features back until it ends (mitcad#750): the bodies at the marker
    // are not the design's.
    refuse(tr("Finish the sketch before exporting: editing it rolls the later features back."));
  }
  if (m_closePending || QApplication::activeModalWidget() != nullptr) {
    refuse(tr("The document is closing or waiting for a dialog."));
  }
  QString reason;
  const QString path = mcp::agentOutputPath(arguments.value(QStringLiteral("path")).toString(), false, reason);
  if (path.isEmpty()) {
    refuse(reason);
  }
  const QString title = m_filePath.isEmpty() ? QStringLiteral("Untitled") : QFileInfo(m_filePath).completeBaseName();
  const QString folder = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
  // The command and the checks are the headless server's (core/ffi/src/mcp.rs).
  QJsonObject plan;
  try {
    plan = parseObject(idleDocument().mcp_export_plan(rustStr(compactJson(arguments)), rustStr(path.toUtf8()),
                                                      rustStr(title.toUtf8()), rustStr(folder.toUtf8())));
  } catch (const std::exception& error) {
    refuse(errorText(error));
  }
  // The other files it writes (an OBJ's .mtl) by the same rule.
  for (const QJsonValue& output : plan.value(QStringLiteral("outputs")).toArray()) {
    if (mcp::agentOutputPath(output.toString(), false, reason).isEmpty()) {
      refuse(reason);
    }
  }
  const QJsonObject command = plan.value(QStringLiteral("command")).toObject();
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  QJsonObject result;
  if (plan.value(QStringLiteral("drawing")).toBool()) {
    // A drawing checks each sheet's file before it writes any.
    try {
      result = parseObject(idleDocument().mcp_export_drawing(rustStr(compactJson(command))));
    } catch (const std::exception& error) {
      refuse(errorText(error));
    }
  } else if (!runCommand(command, &result)) {
    refuse(lastError().isEmpty() ? tr("The export failed.") : lastError());
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent exported %1 (%2)")
                           .arg(path, arguments.value(QStringLiteral("format")).toString());
  return {{QStringLiteral("document"), m_mcpDocumentId},
          {QStringLiteral("path"), path},
          {QStringLiteral("exported"), true},
          {QStringLiteral("result"), result}};
}

void MainWindow::mcpLabelAgentChange(int depthBefore) {
  // The steps an agent added become one, labelled "... (agent)" in the
  // undo text and the version history (mitcad#265); they undo as one.
  try {
    const QJsonObject document = queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
    const int depth = document.value(QStringLiteral("undo_depth")).toInt();
    const QString label = document.value(QStringLiteral("undo")).toString();
    if (depth <= depthBefore || label.isEmpty() || label.endsWith(kAgentSuffix)) {
      return;
    }
    if (runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                    {QStringLiteral("depth"), depthBefore},
                    {QStringLiteral("label"), label + kAgentSuffix}})) {
      qInfo().noquote() << QStringLiteral("Agent change: %1").arg(label + kAgentSuffix);
      updateActions();
    }
  } catch (const std::exception&) {
    // The label stays as the command left it.
  }
}

QJsonObject MainWindow::mcpSave(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // As File > Save does (mitcad#265): to the document's own path, with the
  // project's version and commit message; with a path as Save As does, into
  // a folder for designs (mitcad#402).
  if (m_session || m_mode == Mode::Command) {
    refuse(tr("Finish the current command before saving."));
  }
  if (m_closePending || QApplication::activeModalWidget() != nullptr) {
    refuse(tr("The design is closing or waiting for a dialog."));
  }
  if (windowReadOnly()) {
    refuse(readOnlyReason());
  }
  QString target;
  if (arguments.contains(QStringLiteral("path"))) {
    QString reason;
    target = mcp::agentDesignPath(arguments.value(QStringLiteral("path")).toString(), true,
                                  arguments.value(QStringLiteral("overwrite")).toBool(false), reason);
    if (target.isEmpty()) {
      refuse(reason);
    }
  } else if (m_filePath.isEmpty()) {
    refuse(tr("%1 has never been saved: it has no path to save to. Give a path in a folder for designs "
              "(document_list lists them), or save it in the application (File › Save As).")
               .arg(documentName()));
  }
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  bool saved = false;
  QString dialog;
  {
    mcp::ModalGuard guard;
    saved = target.isEmpty() ? save() : writeFile(target);
    dialog = guard.opened();
  }
  if (!dialog.isEmpty()) {
    refuse(tr("Saving asked a question (%1), which an agent cannot answer: save in the application.").arg(dialog));
  }
  if (!saved || isModified()) {
    refuse(m_statusLabel->text().isEmpty() ? tr("The design was not saved.") : m_statusLabel->text());
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent saved %1").arg(m_filePath);
  return {{QStringLiteral("document"), m_mcpDocumentId},
          {QStringLiteral("path"), m_filePath},
          {QStringLiteral("saved"), true},
          {QStringLiteral("status"), m_statusLabel->text()},
          {QStringLiteral("state"), query({{QStringLiteral("query"), QStringLiteral("document")}})}};
}

QJsonObject MainWindow::mcpOpen(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // As File > Open does, without its dialogs (mitcad#402): a design from a
  // folder for designs; the window's document is replaced and
  // with it the handle.
  QString reason;
  const QString path = mcp::agentDesignPath(arguments.value(QStringLiteral("path")).toString(), false, false, reason);
  if (path.isEmpty()) {
    refuse(reason);
  }
  if (!m_session && m_mode == Mode::PickPlane) {
    refuse(tr("Create Sketch waits for a plane to be picked: pick one or cancel it first."));
  }
  if (m_session || m_mode != Mode::Idle) {
    refuse(tr("Finish the sketch or command first (ui_command with finish ok or cancel)."));
  }
  // What else keeps a model command from running: drags, imports, dialogs,
  // a native command form.
  const QString blocked = mcpBlockedReason(false);
  if (!blocked.isEmpty()) {
    refuse(blocked);
  }
  const bool discard = arguments.value(QStringLiteral("discard")).toBool(false);
  if (!discard && (windowReadOnly() ? m_locks->keptChanges() : isModified())) {
    refuse(tr("%1 has unsaved changes: save them first (document_save), or pass discard: true to lose them.")
               .arg(documentName()));
  }
  const QString before = m_mcpDocumentId;
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  QString error;
  bool opened = false;
  QString dialog;
  {
    mcp::ModalGuard guard;
    opened = openFile(path, error);
    dialog = guard.opened();
  }
  if (!dialog.isEmpty()) {
    refuse(tr("Opening asked a question (%1), which an agent cannot answer: open it in the application.").arg(dialog));
  }
  if (!opened || m_mcpDocumentId == before) {
    refuse(error.isEmpty() ? tr("%1 was not opened.").arg(QDir::toNativeSeparators(path))
                           : tr("Could not open %1: %2").arg(QDir::toNativeSeparators(path), error));
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent opened %1").arg(m_filePath);
  QJsonObject result = mcpDocumentInfo();
  result.insert(QStringLiteral("opened"), true);
  result.insert(QStringLiteral("replaced"), before);
  return result;
}

void MainWindow::setMcpAgents(const QList<mcp::ConnectedAgent>& agents,
                              std::function<void(const QStringList& ids)> disconnect) {
  m_mcpDisconnect = std::move(disconnect);
  if (agents == m_agentIndicator->agents()) {
    return;
  }
  const bool before = m_mcpAgents > 0;
  // ui_state's agent_clients keeps the clients' own labels; people see the
  // assistants' names.
  QStringList names;
  QStringList shown;
  for (const mcp::ConnectedAgent& agent : agents) {
    names << agent.name;
    if (const QString name = mcp::assistantDisplayName(agent.name); !shown.contains(name)) {
      shown << name;
    }
  }
  m_mcpAgentNames = names;
  m_mcpAgents = int(agents.size());
  m_agentIndicator->setAgents(agents);
  if (m_mcpAgents > 0 && !before) {
    qInfo().noquote() << QStringLiteral("Agent connected");
    showHint(tr("%1 connected.").arg(shown.join(QStringLiteral(", "))));
  } else if (m_mcpAgents == 0 && before) {
    qInfo().noquote() << QStringLiteral("Agent disconnected");
    showHint(tr("The AI assistant disconnected."));
  }
}

void MainWindow::disconnectAllMcpAgents() {
  if (!m_mcpDisconnect) {
    return;
  }
  QStringList ids;
  for (const mcp::ConnectedAgent& agent : m_agentIndicator->agents()) {
    ids << agent.id;
  }
  m_mcpDisconnect(ids);
}

void MainWindow::setMcpListener(const QString& instance, int port) {
  m_agentIndicator->setListener(instance, port);
}

void MainWindow::setMcpUnavailable(const QString& reason) {
  m_agentIndicator->setUnavailable(reason);
}

QJsonObject MainWindow::mcpTool(const QString& name, const QJsonObject& arguments) {
  Q_ASSERT(QThread::currentThread() == thread());
  if (name == QStringLiteral("document_list")) {
    // With the folders document_save's path and document_open may use
    // (mitcad#402).
    QJsonArray folders;
    for (const QString& folder : mcp::agentFolders()) {
      folders.append(QDir::toNativeSeparators(folder));
    }
    return {{QStringLiteral("documents"), QJsonArray{mcpDocumentInfo()}}, {QStringLiteral("folders"), folders}};
  }
  // The window's document replaced: no handle (mitcad#402).
  if (name == QStringLiteral("document_open")) {
    return mcpOpen(arguments);
  }
  // What the window shows: no document handle.
  if (name == QStringLiteral("ui_state")) {
    return mcpUiState();
  }
  if (name == QStringLiteral("ui_log")) {
    return mcpLog(arguments);
  }
  if (name == QStringLiteral("ui_screenshot")) {
    return mcpScreenshot(arguments);
  }
  if (name == QStringLiteral("ui_camera")) {
    return mcpCamera(arguments);
  }
  if (arguments.value(QStringLiteral("document")).toString() != m_mcpDocumentId) {
    throw std::runtime_error("No open document with this handle; call document_list after New or Open");
  }
  if (name == QStringLiteral("model_query")) {
    return mcpQuery(arguments.value(QStringLiteral("query")).toObject());
  }
  if (name == QStringLiteral("model_command")) {
    return mcpCommand(arguments);
  }
  if (name == QStringLiteral("model_export")) {
    return mcpExport(arguments);
  }
  if (name == QStringLiteral("document_save")) {
    return mcpSave(arguments);
  }
  if (name == QStringLiteral("ui_select")) {
    return mcpSelect(arguments);
  }
  if (name == QStringLiteral("ui_command")) {
    return mcpUiCommand(arguments);
  }
  throw std::runtime_error("Unknown live document tool");
}

} // namespace mitcad
