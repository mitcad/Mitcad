// SPDX-License-Identifier: MIT
// Exploded views in the window (mitcad#2113; commands.md, "Exploded views";
// app/COMMANDS.md, "Exploded views"):
//   - Explode (the Assemble group): the Explode panel (motion/
//     ExplodeViewDialog) of the first view, else of none yet: its steps,
//     Auto Explode and the play bar. Explode Step (commands/
//     ExplodeCommands.cpp) makes and edits a step.
//   - A view shown exploded: the Browser's row (a double-click, Show
//     Exploded and Show Assembled) or the panel. The bodies are drawn at
//     the explode's places (the model's `explode_view` query with
//     `placements`) and its trail lines over them, at the panel's time;
//     selection and appearance work on what is shown, and so does Measure
//     (mitcad#2328, CommandDef::keepsExplode: the view stays exploded while
//     its panel is open, and the `measure` query gets the view shown, as
//     the selection card's measurements do). Other commands work on the
//     design assembled (their panels show it so); after them the refresh
//     shows it exploded again.
#include "MainWindow.hpp"

#include <cstddef>
#include <exception>
#include <optional>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "commands/CommandFactories.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSession.hpp"
#include "framework/DocumentSession.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "motion/ExplodeViewDialog.hpp"

namespace mitcad {

void MainWindow::registerExplodeCommands() {
  CommandDef explode;
  explode.id = QStringLiteral("assemble.explode");
  explode.name = tr("Explode…");
  explode.icon = QStringLiteral("explode");
  explode.kind = CommandDef::Kind::Action;
  explode.mode = CommandDef::Mode::Model;
  explode.tooltip = tr("Moves components apart in steps, with trail lines, and plays the steps");
  explode.keywords = {QStringLiteral("explode"), QStringLiteral("exploded view"), QStringLiteral("apart"),
                      QStringLiteral("assembly instructions"), QStringLiteral("trail lines")};
  explode.run = [this] { openExplodeView(QString()); };
  explode.enabled = [this] {
    return !m_snapshot.explodeViews.isEmpty() || !m_snapshot.occurrences.isEmpty();
  };
  m_registry->add(explode);
  m_registry->add(cmd::explodeStepCommand(*this));
}

void MainWindow::openExplodeView(const QString& name) {
  if (m_mode != Mode::Idle || m_session) {
    showStatus(tr("Close the command to open the Explode panel."), NoticeLevel::Error);
    return;
  }
  // Its playback moves the bodies shown, as an animation's frames do.
  stopAnimation(QStringLiteral("the Explode panel opened"));
  if (m_explodeDialog) {
    auto* open = static_cast<ExplodeViewDialog*>(m_explodeDialog.data());
    if (!name.isEmpty() && name != open->view()) {
      open->showView(name);
    }
    open->raise();
    open->activateWindow();
    return;
  }
  ExplodeViewHost host;
  host.model = this;
  host.run = [this](const QJsonObject& command, QJsonObject* result) { return runModelCommand(command, result); };
  host.show = [this](const QString& view, std::optional<double> time) { showExplodeAt(view, time); };
  host.editStep = [this](const QString& view, int step) { editExplodeStep(view, step); };
  host.ready = [this] { return m_mode == Mode::Idle && !m_session && !modelBusy(); };
  host.status = [this](const QString& message, bool error) {
    showStatus(message, error ? NoticeLevel::Error : NoticeLevel::Information);
  };
  auto* dialog = new ExplodeViewDialog(std::move(host), name, this);
  m_explodeDialog = dialog;
  // The design changed (a step made, Undo): followExplodePanel, after each
  // refresh. (Not on the session's busyChanged: Explode Step's OK ends
  // its job while its panel is still open.)
  dialog->show();
}

void MainWindow::followExplodePanel() {
  if (m_explodeDialog) {
    static_cast<ExplodeViewDialog*>(m_explodeDialog.data())->refresh();
  }
}

void MainWindow::editExplodeStep(const QString& view, int step) {
  const CommandDef* def = m_registry->find(QStringLiteral("assemble.explode_step"));
  if (def == nullptr || m_mode != Mode::Idle) {
    return;
  }
  // `name` for the session's log line only (a new view: "a new view").
  QJsonObject entry{{QStringLiteral("view"), view},
                    {QStringLiteral("step"), step},
                    {QStringLiteral("name"), view.isEmpty() ? QStringLiteral("a new view") : view}};
  if (step >= 0) {
    const QJsonArray steps = m_snapshot.explodeView(view).value(QStringLiteral("steps")).toArray();
    entry.insert(QStringLiteral("entry"), steps.at(step).toObject());
  }
  qDebug().noquote() << QStringLiteral("Explode Step for %1, %2")
                            .arg(view.isEmpty() ? QStringLiteral("a new view") : view,
                                 step < 0 ? QStringLiteral("a new step") : QStringLiteral("step %1").arg(step + 1));
  startFeatureCommand(*def, QString(), entry);
}

void MainWindow::showExplodeView(const QString& name) { showExplodeAt(name, std::nullopt); }

void MainWindow::showExplodeAt(const QString& name, std::optional<double> time) {
  const bool changed = name != m_explodeShown;
  m_explodeShown = name;
  m_explodeTime = time;
  if (changed) {
    qDebug().noquote() << QStringLiteral("Explode view shown: %1").arg(name.isEmpty() ? QStringLiteral("none") : name);
    if (m_controller != nullptr) {
      m_controller->setShownExplodeView(name);
    }
  }
  if (m_mode != Mode::Idle || m_session || modelBusy()) {
    return; // shown when the command closes
  }
  if (changed) {
    showBodies();
    return;
  }
  // A frame of the playback: the bodies placed again, nothing else.
  std::vector<BodyDisplay> bodies = modelBodies();
  explodeBodies(bodies);
  m_viewer->setBodies(bodies);
}

bool MainWindow::explodeDisplayed() const {
  if (m_explodeShown.isEmpty()) {
    return false;
  }
  if (m_mode == Mode::Idle && !m_session) {
    return true;
  }
  // A command that works on the bodies where the view shows them (Measure,
  // mitcad#2328), outside sketch mode; the others show the design
  // assembled, as they change it.
  return m_mode == Mode::Command && m_session && !m_commandInSketch && m_session->def().keepsExplode;
}

QJsonObject MainWindow::shownExplode() const {
  if (!explodeDisplayed()) {
    return QJsonObject();
  }
  QJsonObject shown{{QStringLiteral("name"), m_explodeShown}};
  if (m_explodeTime) {
    shown.insert(QStringLiteral("time"), *m_explodeTime);
  }
  return shown;
}

void MainWindow::explodeBodies(std::vector<BodyDisplay>& bodies) {
  if (!explodeDisplayed()) {
    m_viewer->setExplodeTrails({});
    m_loggedExplodeKept.clear();
    if (m_explodeShown.isEmpty() && m_loggedExplode != QLatin1String("none")) {
      m_loggedExplode = QStringLiteral("none");
    }
    return;
  }
  // For UI tests: a command's panel that keeps the explode shown.
  const QString kept = m_session ? m_session->def().name : QString();
  if (kept != m_loggedExplodeKept) {
    m_loggedExplodeKept = kept;
    if (!kept.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Explode view %1 kept shown for %2").arg(m_explodeShown, kept);
    }
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("explode_view")},
                      {QStringLiteral("name"), m_explodeShown},
                      {QStringLiteral("placements"), true}};
  if (m_explodeTime) {
    request.insert(QStringLiteral("time"), *m_explodeTime);
  }
  QJsonObject explode;
  try {
    explode = queryObject(request);
  } catch (const std::exception&) {
    // The view is gone (deleted, undone): the design assembled again.
    m_viewer->setExplodeTrails({});
    m_explodeShown.clear();
    m_explodeTime.reset();
    qDebug().noquote() << QStringLiteral("Explode view shown: none");
    if (m_controller != nullptr) {
      m_controller->setShownExplodeView(QString());
    }
    return;
  }
  QHash<QString, QJsonArray> placed;
  for (const QJsonValue& value : explode.value(QStringLiteral("placements")).toArray()) {
    const QJsonObject p = value.toObject();
    placed.insert(str(p, "path"), p.value(QStringLiteral("transform")).toArray());
  }
  QHash<QString, QJsonArray> placedBodies;
  for (const QJsonValue& value : explode.value(QStringLiteral("body_placements")).toArray()) {
    const QJsonObject p = value.toObject();
    placedBodies.insert(str(p, "path") + QLatin1Char('|') + str(p, "body"),
                        p.value(QStringLiteral("transform")).toArray());
  }
  int moved = 0;
  for (BodyDisplay& body : bodies) {
    const QString occurrence = QString::fromStdString(body.occurrence);
    auto at = placed.constFind(occurrence);
    if (at == placed.cend()) {
      at = placedBodies.constFind(occurrence + QLatin1Char('|') + QString::fromStdString(body.uid));
      if (at == placedBodies.cend()) {
        continue;
      }
    }
    body.placement = isIdentity(at.value()) ? TopLoc_Location() : TopLoc_Location(trsfOf(at.value()));
    ++moved;
  }
  std::vector<float> lines;
  const QJsonArray trails = explode.value(QStringLiteral("design_trails")).toArray();
  lines.reserve(static_cast<std::size_t>(trails.size()) * 6);
  for (const QJsonValue& value : trails) {
    const QJsonArray ends = value.toArray();
    for (const QJsonValue& end : ends) {
      const QJsonArray p = end.toArray();
      for (int i = 0; i < 3; ++i) {
        lines.push_back(static_cast<float>(p.at(i).toDouble()));
      }
    }
  }
  m_viewer->setExplodeTrails(lines);
  // For UI tests: what the view shows of it, when that changes.
  const QJsonValue time = explode.value(QStringLiteral("time"));
  const QString logged = QStringLiteral("Explode view %1 at %2: %3 bod%4 moved, %5 trail line(s)")
                             .arg(m_explodeShown,
                                  time.isDouble() ? QStringLiteral("%1 s").arg(time.toDouble(), 0, 'f', 2)
                                                  : QStringLiteral("the end"))
                             .arg(moved)
                             .arg(moved == 1 ? QStringLiteral("y") : QStringLiteral("ies"))
                             .arg(trails.size());
  if (logged != m_loggedExplode) {
    m_loggedExplode = logged;
    qDebug().noquote() << logged;
  }
}

} // namespace mitcad
