// SPDX-License-Identifier: MIT
// Forms in the main window (mitcad#1757; app/COMMANDS.md "Form"): Edit
// Form registered with what it needs of the window (commands/
// FormCommands.cpp), opened for a Form feature from the timeline, the
// Browser or Edit Feature (editFeature), and the cage drawn and picked
// over the view while it is open (view/FormCageOverlay.hpp): a click on the
// cage sets the panel's hidden picks, a vertex's drag its hidden drag
// (mitcad#1810), the panel's changes redraw the cage. Fill Hole and Bridge
// (mitcad#1941) are also actions while the panel is open, in the view's
// context menu and the command search: each chooses its edit in the panel
// and applies it to the picks.
#include "MainWindow.hpp"

#include <exception>
#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "commands/Commands.hpp"
#include "commands/FormCage.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "view/FormCageOverlay.hpp"

namespace mitcad {

void MainWindow::registerFormCommands() {
  FormEditHooks hooks;
  hooks.form = [this] { return m_formEditUid; };
  // Previews change nothing (`preview`), so they run as they are, without
  // the refresh and the volume log of runModelCommand.
  hooks.preview = [this](const QJsonObject& command) { return modelCommand(command); };
  hooks.placement = [this] { return formPlacement(); };
  hooks.show = [this](const QJsonObject& cage, const QString& picked, const QString& kind, bool picking) {
    if (!m_formOverlay) {
      return;
    }
    const std::optional<gp_Trsf> placement = formPlacement();
    m_formOverlay->setCage(forms::cageOf(cage), placement ? *placement : gp_Trsf());
    m_formOverlay->setPicked(forms::pickedOf(picked), forms::kindOf(kind), picking);
  };
  hooks.ended = [this] { endFormEdit(); };
  registerFormEditCommand(*m_registry, *this, *this, std::move(hooks));

  const auto edit = [this](const char* id, const QString& name, const QString& tooltip, const QString& action,
                           const QStringList& keywords) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QStringLiteral("form");
    def.tooltip = tooltip;
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Model;
    def.tab.clear(); // the context menu and the command search
    def.group = QStringLiteral("CREATE");
    def.keywords = keywords;
    def.duringCommands = true;
    def.enabled = [this] { return formEditOpen(); };
    def.run = [this, action] { applyFormEdit(action); };
    m_registry->add(def);
  };
  edit(kFormFillHoleCommand, tr("Fill Hole"), tr("Fills the opening of the picked edge of the Form's cage"),
       QStringLiteral("fill_hole"), {QStringLiteral("form"), QStringLiteral("cage"), QStringLiteral("close"),
                                     QStringLiteral("cap"), QStringLiteral("opening")});
  edit(kFormBridgeCommand, tr("Bridge"), tr("Joins the two openings of the picked edges of the Form's cage"),
       QStringLiteral("bridge"), {QStringLiteral("form"), QStringLiteral("cage"), QStringLiteral("join"),
                                  QStringLiteral("connect"), QStringLiteral("opening")});
}

bool MainWindow::formEditOpen() const {
  return m_mode == Mode::Command && m_session && m_session->def().id == QString::fromLatin1(kFormEditCommand);
}

void MainWindow::applyFormEdit(const QString& action) {
  if (!formEditOpen()) {
    return;
  }
  qDebug().noquote() << QStringLiteral("Form edit %1 asked for: Edit chosen, then Apply").arg(action);
  if (!chooseInCommand(QString::fromLatin1(kFormEditCommand), QStringLiteral("action"), action)) {
    return;
  }
  // Apply once the choice is taken (queued while the model computes): the
  // edit on the picks, one undo step, or why not in words.
  whenIdle(this, [this] {
    if (formEditOpen()) {
      confirm();
    }
  });
}

std::optional<gp_Trsf> MainWindow::formPlacement() const {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(m_formEditUid);
  if (feature == nullptr) {
    return std::nullopt;
  }
  const QString occurrence = m_snapshot.firstOccurrence(feature->component);
  if (occurrence.isEmpty()) {
    return std::nullopt; // the root's: as it is
  }
  return placementOf(occurrence).Transformation();
}

void MainWindow::editForm(const QString& uid) {
  const CommandDef* def = m_registry->find(QString::fromLatin1(kFormEditCommand));
  if (def == nullptr || m_mode != Mode::Idle) {
    return;
  }
  endFormEdit(); // one at a time
  m_formEditUid = uid;
  // Before the panel starts: its init draws the cage.
  m_formOverlay = new FormCageOverlay(*m_viewer);
  connect(m_formOverlay, &FormCageOverlay::pickRequested, this, [this](const QString& picked) {
    if (!chooseInCommand(QString::fromLatin1(kFormEditCommand), QStringLiteral("picked"), picked)) {
      qDebug().noquote() << QStringLiteral("Form cage pick not taken: Edit Form is not open");
    }
  });
  // A vertex dragged in the view (mitcad#1810): the panel puts it there as
  // one undo step (its hidden input `drag`).
  connect(m_formOverlay, &FormCageOverlay::dragRequested, this, [this](const QString& drag) {
    if (!chooseInCommand(QString::fromLatin1(kFormEditCommand), QStringLiteral("drag"), drag)) {
      qDebug().noquote() << QStringLiteral("Form cage drag not taken: Edit Form is not open");
    }
  });
  qInfo().noquote() << QStringLiteral("Editing Form %1 (%2)").arg(uid, featureName(uid));
  startFeatureCommand(*def);
  if (m_mode != Mode::Command || !m_session || m_session->def().id != QString::fromLatin1(kFormEditCommand)) {
    // The panel did not open (a read-only design): nothing is drawn.
    endFormEdit();
  }
}

void MainWindow::endFormEdit() {
  m_formEditUid.clear();
  if (m_formOverlay) {
    m_formOverlay->hide();
    m_formOverlay->deleteLater();
  }
  m_formOverlay = nullptr;
}

// A body of a base feature carries the cage its faces were made from
// (commands.md, "Control cages of imported Form bodies"): its entry in the
// feature's `bodies`, by the body's index (F4.b0: the first).
bool MainWindow::hasKeptCage(const QString& body) const {
  const QString feature = body.section(QLatin1Char('.'), 0, 0);
  const DocumentSnapshot::Feature* made = m_snapshot.feature(feature);
  if (made == nullptr || made->type != QStringLiteral("base") || modelBusy()) {
    return false;
  }
  const QString index = body.section(QLatin1Char('.'), 1, 1);
  bool number = false;
  const int at = index.startsWith(QLatin1Char('b')) ? index.mid(1).toInt(&number) : -1;
  if (!number || at < 0) {
    return false;
  }
  try {
    const QJsonArray bodies = queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), feature}})
                                  .value(QStringLiteral("def"))
                                  .toObject()
                                  .value(QStringLiteral("bodies"))
                                  .toArray();
    return at < bodies.size() && bodies.at(at).toObject().value(QStringLiteral("cage")).isObject();
  } catch (const std::exception&) {
    return false;
  }
}

void MainWindow::openKeptCage(const QString& body) {
  if (m_mode != Mode::Idle) {
    return;
  }
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  const QJsonObject command{
      {QStringLiteral("cmd"), QStringLiteral("form.create")},
      {QStringLiteral("primitive"),
       QJsonObject{{QStringLiteral("type"), QStringLiteral("kept_cage")}, {QStringLiteral("body"), body}}}};
  QJsonObject result;
  // A cage that does not map is refused in words, shown as the model's
  // error; nothing changes then.
  if (!runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("Open Cage as Form of %1 refused: %2").arg(body, lastError());
    return;
  }
  const QString uid = result.value(QStringLiteral("uid")).toString();
  qInfo().noquote() << QStringLiteral("Opened the cage of %1 as %2 (%3)")
                           .arg(body, uid, result.value(QStringLiteral("name")).toString());
  if (result.contains(QStringLiteral("error"))) {
    // The model's own words stay with the feature in the timeline.
    showStatus(tr("The Form was added, but its surface could not be made: the timeline shows why."),
               NoticeLevel::Warning);
  }
  if (!uid.isEmpty()) {
    // Once the menu's action is over: the Form opens in Edit Form.
    whenIdle(this, [this, uid] { editFeature(uid); });
  }
}

} // namespace mitcad
