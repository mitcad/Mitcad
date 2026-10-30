// SPDX-License-Identifier: MIT
#include "BrowserController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <iterator>
#include <optional>
#include <tuple>
#include <utility>

#include <QColor>
#include <QColorDialog>
#include <QEvent>
#include <QInputDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QPair>
#include <QPushButton>
#include <QSet>
#include <QToolButton>
#include <QWidget>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/MoveOccurrenceDialog.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../files/Libraries.hpp"
#include "../library/EditOptionsDialog.hpp"
#include "../library/PartOptions.hpp"
#include "DocumentHost.hpp"
#include "AppearanceDialog.hpp"
#include "Bindings.hpp"
#include "BindingsCard.hpp"
#include "ConfigurationBindingsPage.hpp"
#include "ConfigurationBar.hpp"
#include "ConfigurationsDialog.hpp"
#include "FeatureStatus.hpp"
#include "FindDialog.hpp"
#include "ParametersDialog.hpp"
#include "PolicyDialog.hpp"
#include "ReorientDialog.hpp"
#include "TimelineWidget.hpp"

namespace mitcad {
namespace {

QJsonObject cmd(const char* name, QJsonObject fields = {}) {
  fields.insert(QStringLiteral("cmd"), QString::fromLatin1(name));
  return fields;
}

// A context menu whose entries are logged ("Context menu: A | B") for the
// UI tests, which choose entries by their place.
class Menu {
public:
  explicit Menu(QWidget* parent) : m_menu(parent) {}

  void add(const QString& icon, const QString& text, std::function<void()> call) {
    QAction* action = m_menu.addAction(themeIcon(icon), text);
    QObject::connect(action, &QAction::triggered, &m_menu, std::move(call));
    m_entries << text;
  }
  // An entry that cannot be chosen while `blocked` says why (its tooltip).
  void add(const QString& icon, const QString& text, std::function<void()> call, const QString& blocked) {
    QAction* action = m_menu.addAction(themeIcon(icon), text);
    QObject::connect(action, &QAction::triggered, &m_menu, std::move(call));
    m_entries << text;
    if (!blocked.isEmpty()) {
      action->setEnabled(false);
      action->setToolTip(blocked);
      m_menu.setToolTipsVisible(true);
    }
  }
  // An entry with a tooltip that says what it leaves out (mitcad#863).
  void addWithTip(const QString& icon, const QString& text, std::function<void()> call, const QString& tip) {
    QAction* action = m_menu.addAction(themeIcon(icon), text);
    QObject::connect(action, &QAction::triggered, &m_menu, std::move(call));
    m_entries << text;
    if (!tip.isEmpty()) {
      action->setToolTip(tip);
      m_menu.setToolTipsVisible(true);
    }
  }
  // An entry for a property that can follow an expression: while it does
  // (`bound`), an fx mark on its icon and after its text ("Appearance…
  // fx", mitcad#861), logged "Context menu fx: Appearance…"; the entry is
  // logged by its text alone either way.
  void addBound(const QString& icon, const QString& text, std::function<void()> call, bool bound) {
    QAction* action = m_menu.addAction(bound ? boundIcon(themeIcon(icon)) : themeIcon(icon),
                                       bound ? text + QStringLiteral("\tfx") : text);
    QObject::connect(action, &QAction::triggered, &m_menu, std::move(call));
    m_entries << text;
    if (bound) {
      m_bound << text;
    }
  }
  void separator() { m_menu.addSeparator(); }
  bool isEmpty() const { return m_entries.isEmpty(); }
  void exec(const QPoint& at) {
    if (!m_bound.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Context menu fx: %1").arg(m_bound.join(QStringLiteral(" | ")));
    }
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(m_entries.join(QStringLiteral(" | ")));
    execMenu(&m_menu, at);
  }

private:
  QMenu m_menu;
  QStringList m_entries;
  QStringList m_bound;
};

const QStringList kLengthUnits = {QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"),
                                  QStringLiteral("in"), QStringLiteral("ft")};

} // namespace

template <typename Sender, typename... Args>
void BrowserController::follow(Sender* sender, void (Sender::*signal)(Args...),
                               void (BrowserController::*slot)(Args...), bool dropWhenBusy) {
  connect(
      sender, signal, this,
      [this, slot, dropWhenBusy](Args... args) {
        if (!m_host.modelBusy()) {
          (this->*slot)(args...);
        } else if (!dropWhenBusy) {
          m_host.whenIdle(this, [this, slot, args...] { (this->*slot)(args...); });
        }
      },
      Qt::QueuedConnection);
}

BrowserController::BrowserController(DocumentHost& host, QWidget* window)
    : QObject(window), m_host(host), m_window(window) {
  m_browser = new BrowserPanel(window);
  // The Configuration bar above the rows (mitcad#807): hidden while the
  // design has no options.
  m_configurationBar = new ConfigurationBar(host, m_browser);
  m_browser->setTopBar(m_configurationBar);
  m_timeline = new TimelineWidget(host.model(), window);
  m_failures = new QToolButton(window);
  m_failures->setObjectName(QStringLiteral("failuresBadge"));
  m_failures->setFocusPolicy(Qt::NoFocus);
  m_failures->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  // The timeline's badges (mitcad#961): a red circle, an amber triangle.
  m_failures->setIcon(featureStateIcon(FeatureState::Failed));
  // A red capsule with white text, readable on any background (mitcad#961).
  m_failures->setAutoRaise(false);
  m_failures->setStyleSheet(issueBadgeStyle(FeatureState::Failed));
  m_failures->setProperty(kSemanticTextToneProperty, QStringLiteral("error"));
  m_failures->hide();
  // Features with warnings, the import's among them (mitcad#170): yellow.
  m_warnings = new QToolButton(window);
  m_warnings->setObjectName(QStringLiteral("warningsBadge"));
  m_warnings->setFocusPolicy(Qt::NoFocus);
  m_warnings->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_warnings->setIcon(featureStateIcon(FeatureState::Warning));
  m_warnings->setAutoRaise(false);
  m_warnings->setStyleSheet(issueBadgeStyle(FeatureState::Warning));
  m_warnings->setProperty(kSemanticTextToneProperty, QStringLiteral("warning"));
  m_warnings->hide();
  // The magnifier on the browser's title row (mitcad#350): Find in Design.
  m_findButton = new QToolButton(window);
  m_findButton->setObjectName(QStringLiteral("findInDesignButton"));
  m_findButton->setAutoRaise(true);
  m_findButton->setFocusPolicy(Qt::NoFocus);
  m_findButton->setIcon(themeIcon(QStringLiteral("search")));
  m_findButton->setToolTip(tr("Find in Design: components and bodies by name"));
  m_findButton->installEventFilter(this);
  connect(m_findButton, &QToolButton::clicked, this, &BrowserController::openFind);

  follow(m_browser, &BrowserPanel::itemsPicked, &BrowserController::pickItems);
  follow(m_browser, &BrowserPanel::rowsPicked, &BrowserController::pickRows);
  follow(m_browser, &BrowserPanel::renameClicked, &BrowserController::renameClicked, true);
  follow(m_browser, &BrowserPanel::eyeClicked, &BrowserController::toggleVisibility);
  follow(m_browser, &BrowserPanel::activateClicked, &BrowserController::activate);
  follow(m_browser, &BrowserPanel::renamed, &BrowserController::rename);
  follow(m_browser, &BrowserPanel::doubleClicked, &BrowserController::browserDoubleClicked);
  follow(m_browser, &BrowserPanel::deleteRequested, &BrowserController::deleteKeyed);
  follow(m_browser, &BrowserPanel::contextMenuRequested, &BrowserController::browserMenu, true);

  follow(m_timeline, &TimelineWidget::featuresClicked, &BrowserController::featuresClicked);
  follow(m_timeline, &TimelineWidget::editRequested, &BrowserController::editFeature);
  follow(m_timeline, &TimelineWidget::contextMenuRequested, &BrowserController::timelineMenu, true);
  follow(m_timeline, &TimelineWidget::groupMenuRequested, &BrowserController::groupMenu, true);
  follow(m_timeline, &TimelineWidget::deleteRequested, &BrowserController::deleteFeatureKeyed);
  follow(m_timeline, &TimelineWidget::renamed, &BrowserController::renameFeature);
  follow(m_timeline, &TimelineWidget::reorderRequested, &BrowserController::reorder);
  follow(m_timeline, &TimelineWidget::markerRequested, &BrowserController::setMarker);
  // A press: never while a job computes the model, whose caller waits with
  // input queued or blocked.
  connect(m_timeline, &TimelineWidget::markerDragStarted, this, [this] {
    m_markerDepth = canChange() ? undoDepth() : -1;
    m_markerStart = m_snapshot.marker;
  });
  follow(m_timeline, &TimelineWidget::markerDragged, &BrowserController::markerDragged);
  follow(m_timeline, &TimelineWidget::markerDragFinished, &BrowserController::markerDragFinished);
  // The timeline's search (mitcad#179): Ctrl+F in the browser opens it; the
  // selection from before it comes back when it is cleared.
  connect(m_browser, &BrowserPanel::searchRequested, m_timeline, &TimelineWidget::openSearch);
  // The pointer over a joint's row previews its highlight (mitcad#523).
  connect(m_browser, &BrowserPanel::jointHovered, this,
          [this](const QString& uid, const QString& occurrence) { m_host.previewJoint(uid, occurrence); });
  connect(m_timeline, &TimelineWidget::searchStarted, this, [this] {
    m_searchItems = m_shownItems;
    m_searchFeatures = m_shownFeatures;
  });
  follow(m_timeline, &TimelineWidget::searchRestored, &BrowserController::restoreSearchSelection);
  // The fx badges show their bindings cards (mitcad#862); not while a job
  // computes the model.
  follow(m_browser, &BrowserPanel::bindingsRequested, &BrowserController::browserBindings, true);
  follow(m_timeline, &TimelineWidget::bindingsRequested, &BrowserController::timelineBindings, true);

  // A click on a badge goes to its feature, or lists them to go to.
  connect(m_failures, &QToolButton::clicked, this, [this] {
    showIssues(false, m_failures->mapToGlobal(QPoint(0, 0)));
  });
  connect(m_warnings, &QToolButton::clicked, this, [this] {
    showIssues(true, m_warnings->mapToGlobal(QPoint(0, 0)));
  });
}

void BrowserController::showIssues(bool warnings, const QPoint& globalPosition) {
  const auto features = warnings ? m_snapshot.warnings() : m_snapshot.failures();
  if (features.isEmpty()) return;
  if (features.size() == 1) {
    reveal(features.first()->uid);
    return;
  }
  Menu menu(m_window);
  for (const DocumentSnapshot::Feature* feature : features) {
    const QString uid = feature->uid;
    menu.add(featureIcon(feature->type), QStringLiteral("%1: %2").arg(feature->name, feature->error),
             [this, uid] { reveal(uid); });
  }
  menu.exec(globalPosition);
}

// ---------------------------------------------------------------------------
// Keeping in step

void BrowserController::refresh(const DocumentSnapshot& snapshot) {
  const bool origin = m_host.originShown();
  if (!m_shown || snapshot.key != m_snapshot.key || origin != m_originShown) {
    m_shown = true;
    m_snapshot = snapshot;
    m_originShown = origin;
    m_browser->rebuild(snapshot, origin);
    m_timeline->rebuild(snapshot);
    updateFailures();
  }
  if (m_parameters && m_parameters->isVisible()) {
    m_parameters->refresh();
  }
  // Configurations (mitcad#807): the bar always, the window while shown.
  m_configurationBar->refresh();
  if (m_configurations && m_configurations->isVisible()) {
    m_configurations->refresh();
  }
  // A bindings card shown reads its item again (mitcad#862).
  if (m_bindingsCard && m_bindingsCard->isVisible()) {
    m_bindingsCard->refresh();
  }
  if (m_appearances && m_appearances->isVisible()) {
    m_appearances->refresh();
  }
  if (m_propertySets && m_propertySets->isVisible()) {
    m_propertySets->refresh();
  }
  // The print windows (mitcad#655).
  if (m_materials && m_materials->isVisible()) {
    m_materials->refresh();
  }
  if (m_profiles && m_profiles->isVisible()) {
    m_profiles->refresh();
  }
  if (m_ownSettings && m_ownSettings->isVisible()) {
    m_ownSettings->refresh();
  }
  // A Save to or a library edit of the design shows in My Library
  // (mitcad#704).
  if (m_myLibrary && m_myLibrary->isVisible()) {
    m_myLibrary->setProjectFolder(m_host.projectFolder());
    m_myLibrary->refresh();
  }
  if (m_purgeVolumes && m_purgeVolumes->isVisible()) {
    m_purgeVolumes->refresh(); // mitcad#705
  }
  if (m_find) {
    m_find->setSnapshot(m_snapshot);
  }
}

void BrowserController::designOpened() {
  m_shown = false; // rebuilt, also when it reads as the design before did
  m_browser->designOpened();
}

void BrowserController::showSelection(const Selection& items) {
  // A pick of features shows their rows under the sketches too (mitcad#177),
  // also when the same selection is shown again; another selection (in the
  // view) does not.
  if (m_picking) {
    m_shownFeatures = m_pickedFeatures;
  } else if (items != m_shownItems) {
    m_shownFeatures.clear();
  }
  m_shownItems = items;
  // Rows picked in the tree stay where they are: scrolling would jump to
  // another selected row (mitcad#318). A pick in the timeline, the view or
  // a command scrolls to its row when none is in sight.
  m_browser->showSelection(items, m_shownFeatures, !(m_picking && m_fromTree));
}

bool BrowserController::cancelEditing() {
  return m_browser->cancelEditing() || m_timeline->cancelEditing();
}

void BrowserController::updateFailures() {
  const QVector<const DocumentSnapshot::Feature*> failed = m_snapshot.failures();
  QStringList lines;
  for (const DocumentSnapshot::Feature* feature : failed) {
    lines << QStringLiteral("%1: %2").arg(feature->name, feature->error);
  }
  m_failures->setVisible(!failed.isEmpty());
  m_failures->setText(tr("%n failed", nullptr, static_cast<int>(failed.size())));
  m_failures->setToolTip(lines.join(QLatin1Char('\n')));
  const QString logged = lines.join(QStringLiteral("; "));
  if (logged != m_loggedFailures) {
    m_loggedFailures = logged;
    qDebug().noquote() << QStringLiteral("Failed features: %1")
                              .arg(logged.isEmpty() ? QStringLiteral("none") : logged);
    // Where to click, once the status bar has placed it.
    TestSync::singleShot(300, m_failures, [this] {
      if (m_failures->isVisible()) {
        const QPoint at = GlassCard::mapToHost(m_failures, m_failures->rect().center());
        qDebug().noquote() << QStringLiteral("Failures button at %1,%2").arg(at.x()).arg(at.y());
      }
    });
  }
  // Features that succeeded with warnings are yellow in the timeline (P9).
  QStringList warnings;
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (feature.status == QStringLiteral("warning")) {
      warnings << QStringLiteral("%1: %2").arg(feature.name, feature.error);
    }
  }
  const QString warned = warnings.join(QStringLiteral("; "));
  if (warned != m_loggedWarnings) {
    m_loggedWarnings = warned;
    qDebug().noquote() << QStringLiteral("Feature warnings: %1").arg(warned.isEmpty() ? QStringLiteral("none") : warned);
  }
  // The yellow badge beside the failures' (mitcad#170): those, and what the
  // import left a note on (a placeholder of a skipped item is suppressed).
  const QVector<const DocumentSnapshot::Feature*> noted = m_snapshot.warnings();
  QStringList badge;
  for (const DocumentSnapshot::Feature* feature : noted) {
    badge << QStringLiteral("%1: %2").arg(feature->name, feature->error);
  }
  m_warnings->setVisible(!noted.isEmpty());
  const int warnedCount = static_cast<int>(noted.size());
  const QString warnedText = warnedCount == 1 ? tr("1 warning") : tr("%1 warnings").arg(warnedCount);
  m_warnings->setText(m_compactBadges ? QString::number(warnedCount) : warnedText);
  m_warnings->setAccessibleName(warnedText);
  m_warnings->setToolTip(badge.join(QLatin1Char('\n')));
  const QString badged = badge.join(QStringLiteral("; "));
  if (badged != m_loggedBadge) {
    m_loggedBadge = badged;
    qDebug().noquote() << QStringLiteral("Warnings badge: %1").arg(badged.isEmpty() ? QStringLiteral("none") : badged);
    TestSync::singleShot(300, m_warnings, [this] {
      if (m_warnings->isVisible()) {
        const QPoint at = GlassCard::mapToHost(m_warnings, m_warnings->rect().center());
        qDebug().noquote() << QStringLiteral("Warnings button at %1,%2").arg(at.x()).arg(at.y());
      }
    });
  }
  // The sketches' degrees of freedom the browser shows (P9).
  QStringList dofs;
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (feature.isSketch() && feature.dof >= 0) {
      dofs << QStringLiteral("%1 %2").arg(feature.name).arg(feature.dof);
    }
  }
  const QString dof = dofs.join(QStringLiteral(", "));
  if (dof != m_loggedDof) {
    m_loggedDof = dof;
    qDebug().noquote() << QStringLiteral("Browser sketch DOF: %1").arg(dof.isEmpty() ? QStringLiteral("none") : dof);
  }
  // The joints (mitcad#55): each one's state, and the degrees of freedom of
  // each component with joints.
  QStringList joints;
  for (const QJsonValue& value : m_snapshot.joints.value(QStringLiteral("joints")).toArray()) {
    const QJsonObject joint = value.toObject();
    joints << QStringLiteral("%1 %2").arg(joint.value(QStringLiteral("name")).toString(),
                                          joint.value(QStringLiteral("state")).toString());
  }
  // Motion links (mitcad#466): "MotionLink1 gear placed".
  for (const QJsonValue& value : m_snapshot.joints.value(QStringLiteral("links")).toArray()) {
    const QJsonObject link = value.toObject();
    joints << QStringLiteral("%1 %2 %3").arg(link.value(QStringLiteral("name")).toString(),
                                             link.value(QStringLiteral("coupling")).toString(),
                                             link.value(QStringLiteral("state")).toString());
  }
  for (const QJsonValue& value : m_snapshot.joints.value(QStringLiteral("dof")).toArray()) {
    const QJsonObject component = value.toObject();
    joints << QStringLiteral("DOF %1 %2")
                  .arg(m_snapshot.componentName(component.value(QStringLiteral("component")).toString()))
                  .arg(component.value(QStringLiteral("total")).toInt());
  }
  const QString jointLog = joints.join(QStringLiteral(", "));
  if (jointLog != m_loggedJoints) {
    m_loggedJoints = jointLog;
    qDebug().noquote() << QStringLiteral("Browser joints: %1").arg(jointLog.isEmpty() ? QStringLiteral("none") : jointLog);
  }
}

void BrowserController::reveal(const QString& uid) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr) {
    return;
  }
  m_timeline->selectFeature(uid);
  m_browser->reveal(uid);
  // Selected as a click on its cell selects it, so that the selection card
  // names it and why it failed (mitcad#961); not into a running command's
  // input.
  if (m_host.activeCommandName().isEmpty()) {
    pick(featureItems({uid}), m_host.takesFeatures() ? QStringList() : QStringList{uid});
  }
  if (!feature->error.isEmpty()) {
    m_host.showStatus(QStringLiteral("%1: %2").arg(feature->name, feature->error), true);
  }
  qDebug().noquote() << QStringLiteral("Revealed %1").arg(feature->name);
}

bool BrowserController::canChange(bool visibilityOnly) {
  if (m_host.canChangeModel(visibilityOnly)) {
    return true;
  }
  const QString readOnly = m_host.readOnlyReason();
  m_host.showStatus(readOnly.isEmpty() ? m_host.changeBlockedReason() : readOnly, true);
  return false;
}

int BrowserController::undoDepth() const {
  try {
    return m_host.model()
        .queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
        .value(QStringLiteral("undo_depth"))
        .toInt();
  } catch (const std::exception&) {
    return 0;
  }
}

const DocumentSnapshot::Occurrence* BrowserController::occurrence(const QString& path) const {
  std::function<const DocumentSnapshot::Occurrence*(const QVector<DocumentSnapshot::Occurrence>&)> find =
      [&](const QVector<DocumentSnapshot::Occurrence>& occurrences) -> const DocumentSnapshot::Occurrence* {
    for (const DocumentSnapshot::Occurrence& o : occurrences) {
      if (o.path == path) {
        return &o;
      }
      if (const DocumentSnapshot::Occurrence* inside = find(o.children)) {
        return inside;
      }
    }
    return nullptr;
  };
  return find(m_snapshot.occurrences);
}

// ---------------------------------------------------------------------------
// The browser

// The light bulbs of bodies, their folders, sketches, construction geometry,
// components and the Origin folder work in every mode (mitcad#295): while
// Create Sketch asks for a plane, while sketching and during commands, whose
// undo range they stay out of. A Flat Pattern and an analysis change as the
// rest of the model does: outside commands.
bool BrowserController::visibilityInEveryMode(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::Origin:
  case BrowserNode::Type::Body:
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
  case BrowserNode::Type::Component:
  case BrowserNode::Type::Bodies:
  case BrowserNode::Type::Sketches:
  case BrowserNode::Type::Construction:
    return true;
  default:
    return false;
  }
}

void BrowserController::toggleVisibility(const BrowserNode& node) {
  const bool show = !node.visible;
  const QString verb = show ? QStringLiteral("shown") : QStringLiteral("hidden");
  if (!visibilityInEveryMode(node) && !canChange(true)) {
    return;
  }
  bool done = false;
  // The light bulbs of every mode: one undo step, outside a command's.
  QJsonArray commands;
  QString label;
  switch (node.type) {
  case BrowserNode::Type::Origin:
    commands.append(cmd("set_origin_visible", {{QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Body:
    commands.append(cmd("set_body_visible", {{QStringLiteral("uid"), node.uid}, {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
    commands.append(cmd("set_feature_visible", {{QStringLiteral("uid"), node.uid},
                                                {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Component:
    commands.append(cmd("set_occurrence_visible",
                        {{QStringLiteral("occurrence"), node.occurrence}, {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Bodies: {
    // Bodies with a Show When condition follow it (mitcad#863): the folder
    // changes the others.
    const QStringList kept = conditionedBodies(node.component);
    const DocumentSnapshot::Component* component = m_snapshot.component(node.component);
    for (const QString& uid : component != nullptr ? component->bodies : QStringList()) {
      if (m_snapshot.bodies.value(uid).visibleWhen.isEmpty()) {
        commands.append(cmd("set_body_visible", {{QStringLiteral("uid"), uid}, {QStringLiteral("visible"), show}}));
      }
    }
    label = show ? tr("Show Bodies") : tr("Hide Bodies");
    if (!kept.isEmpty()) {
      keptByConditions(label, node.path, kept);
      if (commands.isEmpty()) {
        m_host.showStatus(bodiesFollowConditions(kept), false);
        return;
      }
    }
    done = commands.isEmpty(); // an empty folder: nothing to change
    break;
  }
  case BrowserNode::Type::Sketches:
  case BrowserNode::Type::Construction: {
    const bool sketches = node.type == BrowserNode::Type::Sketches;
    for (const DocumentSnapshot::Feature& feature : m_snapshot.features) {
      if (feature.component == node.component && feature.isActive() &&
          (sketches ? feature.isSketch() : feature.isConstruction())) {
        commands.append(cmd("set_feature_visible", {{QStringLiteral("uid"), feature.uid},
                                                    {QStringLiteral("visible"), show}}));
      }
    }
    const QString what = sketches ? tr("Sketches") : tr("Construction");
    label = (show ? tr("Show %1") : tr("Hide %1")).arg(what);
    done = commands.isEmpty();
    break;
  }
  case BrowserNode::Type::FlatPattern:
    // The sheet shown flat in place of the folded body (mitcad#210).
    done = m_host.runModelCommand(cmd("set_flat_pattern", {{QStringLiteral("body"), node.uid},
                                                           {QStringLiteral("shown"), show}}));
    break;
  case BrowserNode::Type::Analysis:
    // Showing it hides the others: one section at a time (mitcad#41).
    done = m_host.runModelCommand(cmd("set_analysis_visible", {{QStringLiteral("name"), node.uid},
                                                               {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::ImageReference: // mitcad#782
    done = m_host.runModelCommand(cmd("set_image_reference_visible", {{QStringLiteral("name"), node.uid},
                                                                      {QStringLiteral("visible"), show}}));
    break;
  default:
    return;
  }
  // A body or an occurrence that follows a Show When condition: the value
  // set by hand removes the condition in the same undo step (the model's
  // rule), which the status bar says (mitcad#863).
  QString condition;
  if (node.type == BrowserNode::Type::Body) {
    condition = m_snapshot.bodies.value(node.uid).visibleWhen;
  } else if (node.type == BrowserNode::Type::Component && !node.occurrence.isEmpty()) {
    condition = occurrenceCondition(node.occurrence);
  }
  if (!commands.isEmpty()) {
    done = m_host.runVisibilityCommands(commands, label);
  }
  if (done) {
    const QString during = m_host.activeCommandName();
    qDebug().noquote() << (during.isEmpty()
                               ? QStringLiteral("Visibility %1: %2").arg(node.path, verb)
                               : QStringLiteral("Visibility %1: %2 (during %3)").arg(node.path, verb, during));
    if (!condition.isEmpty()) {
      conditionRemoved(node.name);
    }
  }
}

QString BrowserController::occurrenceCondition(const QString& path) const {
  const DocumentSnapshot::Occurrence* placed = occurrence(path);
  return placed != nullptr ? placed->visibleWhen : QString();
}

void BrowserController::conditionRemoved(const QString& name) {
  const QString message = tr("Show When of %1 removed – Undo brings it back").arg(name);
  m_host.showStatus(message, false);
  qDebug().noquote() << QStringLiteral("Status: %1").arg(message);
}

void BrowserController::keptByConditions(const QString& label, const QString& where, const QStringList& kept) {
  const QString names = kept.join(QStringLiteral(", "));
  qDebug().noquote() << QStringLiteral("%1%2: kept %3 following Show When: %4")
                            .arg(label, where.isEmpty() ? QString() : QStringLiteral(" %1").arg(where))
                            .arg(kept.size())
                            .arg(names);
  const QString message = tr("%1 left Show When in place for %2").arg(label, names);
  m_host.showStatus(message, false);
  qDebug().noquote() << QStringLiteral("Status: %1").arg(message);
}

QStringList BrowserController::conditionedBodies(const QString& owner) const {
  QStringList names;
  const DocumentSnapshot::Component* def = m_snapshot.component(owner);
  for (const QString& uid : def != nullptr ? def->bodies : QStringList()) {
    const DocumentSnapshot::Body body = m_snapshot.bodies.value(uid);
    if (!body.visibleWhen.isEmpty()) {
      names << (body.name.isEmpty() ? uid : body.name);
    }
  }
  return names;
}

QString BrowserController::leftToConditions(const QStringList& bodies) {
  return bodies.size() == 1
             ? tr("%1 follows its Show When and is left as it is.").arg(bodies.first())
             : tr("%1 follow their Show When and are left as they are.").arg(bodies.join(QStringLiteral(", ")));
}

QString BrowserController::bodiesFollowConditions(const QStringList& bodies) {
  return QStringLiteral("%1 %2").arg(leftToConditions(bodies),
                                     tr("Show When… on a body's row changes its condition."));
}

void BrowserController::activate(const BrowserNode& node) {
  if (node.type != BrowserNode::Type::Component || !canChange()) {
    return;
  }
  if (m_snapshot.activeComponent == node.uid) {
    return;
  }
  if (m_host.runModelCommand(cmd("activate_component", {{QStringLiteral("component"), node.uid}}))) {
    qDebug().noquote() << QStringLiteral("Activated %1").arg(m_snapshot.componentName(node.uid));
  }
}

void BrowserController::rename(const BrowserNode& node, const QString& name) {
  if (!canChange()) {
    m_browser->rebuild(m_snapshot, m_originShown); // puts the old name back
    return;
  }
  QJsonObject command;
  switch (node.type) {
  case BrowserNode::Type::Component:
    command = cmd("rename_component", {{QStringLiteral("component"), node.uid}, {QStringLiteral("name"), name}});
    break;
  case BrowserNode::Type::Body:
    command = cmd("rename_body", {{QStringLiteral("uid"), node.uid}, {QStringLiteral("name"), name}});
    break;
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
  case BrowserNode::Type::Joint:
  case BrowserNode::Type::Feature:
    command = cmd("rename_feature", {{QStringLiteral("uid"), node.uid}, {QStringLiteral("name"), name}});
    break;
  case BrowserNode::Type::NamedView:
    command = cmd("rename_named_view",
                  {{QStringLiteral("name"), node.uid.mid(6)}, {QStringLiteral("new_name"), name}});
    break;
  case BrowserNode::Type::Analysis:
    command = cmd("rename_analysis", {{QStringLiteral("name"), node.uid}, {QStringLiteral("new_name"), name}});
    break;
  case BrowserNode::Type::MotionStudy:
    command = cmd("edit_motion_study", {{QStringLiteral("name"), node.uid}, {QStringLiteral("new_name"), name}});
    break;
  case BrowserNode::Type::ContactSet:
    command = cmd("edit_contact_set", {{QStringLiteral("name"), node.uid}, {QStringLiteral("new_name"), name}});
    break;
  case BrowserNode::Type::Drawing: // mitcad#707
    command = cmd("edit_drawing", {{QStringLiteral("drawing"), node.uid}, {QStringLiteral("name"), name}});
    break;
  case BrowserNode::Type::ImageReference: // mitcad#782
    command = cmd("rename_image_reference", {{QStringLiteral("name"), node.uid}, {QStringLiteral("new_name"), name}});
    break;
  default:
    return;
  }
  QJsonObject result;
  if (m_host.runModelCommand(command, &result)) {
    qDebug().noquote() << QStringLiteral("Renamed %1 to %2").arg(node.name, name);
    // A component's only, default-named body takes its name (mitcad#291);
    // the browser and the timeline show it after the refresh.
    const QJsonObject body = result.value(QStringLiteral("renamed_body")).toObject();
    if (!body.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Renamed body %1 with its component to %2")
                                .arg(body.value(QStringLiteral("uid")).toString(),
                                     body.value(QStringLiteral("name")).toString());
    }
  } else {
    m_browser->rebuild(m_snapshot, m_originShown);
  }
}

void BrowserController::renameFeature(const QString& uid, const QString& name) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr || feature->name == name || !canChange()) {
    return;
  }
  const QString old = feature->name;
  if (m_host.runModelCommand(cmd("rename_feature", {{QStringLiteral("uid"), uid}, {QStringLiteral("name"), name}}))) {
    qDebug().noquote() << QStringLiteral("Renamed %1 to %2").arg(old, name);
  }
}

void BrowserController::browserDoubleClicked(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
  case BrowserNode::Type::Joint:
  case BrowserNode::Type::Feature:
  case BrowserNode::Type::SheetRules:
    m_host.editFeature(node.uid);
    break;
  case BrowserNode::Type::Body:
  case BrowserNode::Type::Component:
    if (canChange()) {
      m_browser->startRename(node);
    }
    break;
  case BrowserNode::Type::Measurements: // the Annotations folder (mitcad#702)
    m_host.openAnnotations();
    break;
  case BrowserNode::Type::FlatPattern:
    editFlatPattern(node.uid);
    break;
  case BrowserNode::Type::Units:
    changeUnits();
    break;
  case BrowserNode::Type::NamedView:
    m_host.showNamedView(node.uid);
    break;
  case BrowserNode::Type::Analysis:
    m_host.editAnalysis(node.uid);
    break;
  case BrowserNode::Type::MotionStudy:
    m_host.openMotionStudy(node.uid);
    break;
  case BrowserNode::Type::ContactSet:
    m_host.editContactSet(node.uid);
    break;
  case BrowserNode::Type::Drawing: // mitcad#707: its sheet over the view
    m_host.openDrawing(node.uid);
    break;
  case BrowserNode::Type::ImageReference: // mitcad#782: its panel
    m_host.editImageReference(node.uid);
    break;
  default:
    break;
  }
}

void BrowserController::editFlatPattern(const QString& body) {
  if (canChange()) {
    m_host.startCommand(QStringLiteral("sheet.flat_pattern"), {SelectionItem{SelectKind::Body, body, QString(), QString()}},
                        nullptr);
  }
}

bool BrowserController::keyDeletesModel() {
  // While a drawing is open the Delete key is its views' (mitcad#707).
  if (!m_host.isDrawingOpen()) {
    return true;
  }
  m_host.showStatus(tr("The Delete key deletes the drawing's views while it is open: close the drawing to "
                       "delete model items."),
                    true);
  return false;
}

void BrowserController::deleteKeyed(const BrowserNode& node) {
  if (node.type == BrowserNode::Type::Drawing || keyDeletesModel()) {
    deleteNode(node);
  }
}

void BrowserController::deleteFeatureKeyed(const QString& uid) {
  if (keyDeletesModel()) {
    deleteFeature(uid);
  }
}

void BrowserController::deleteNode(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::FlatPattern:
    if (canChange() && m_host.runModelCommand(cmd("remove_flat_pattern", {{QStringLiteral("body"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted the flat pattern of %1").arg(m_host.model().bodyName(node.uid));
    }
    break;
  case BrowserNode::Type::Body:
    // The feature that made it goes, as nothing removes a body by itself.
    deleteFeature(node.uid.section(QLatin1Char('.'), 0, 0));
    break;
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
  case BrowserNode::Type::Joint:
  case BrowserNode::Type::Feature:
    deleteFeature(node.uid);
    break;
  case BrowserNode::Type::Component:
    if (!node.occurrence.isEmpty() && canChange()) {
      QJsonObject result;
      if (m_host.runModelCommand(cmd("delete_occurrence", {{QStringLiteral("occurrence"), node.occurrence}}),
                                 &result)) {
        qDebug().noquote() << QStringLiteral("Deleted %1").arg(node.name);
      }
    }
    break;
  case BrowserNode::Type::NamedView:
    if (node.uid.startsWith(QStringLiteral("named:")) && canChange() &&
        m_host.runModelCommand(cmd("delete_named_view", {{QStringLiteral("name"), node.uid.mid(6)}}))) {
      qDebug().noquote() << QStringLiteral("Deleted named view %1").arg(node.name);
    }
    break;
  case BrowserNode::Type::Analysis:
    if (canChange() && m_host.runModelCommand(cmd("delete_analysis", {{QStringLiteral("name"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted analysis %1").arg(node.name);
    }
    break;
  case BrowserNode::Type::MotionStudy:
    if (canChange() && m_host.runModelCommand(cmd("delete_motion_study", {{QStringLiteral("name"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted motion study %1").arg(node.name);
    }
    break;
  case BrowserNode::Type::ContactSet:
    if (canChange() && m_host.runModelCommand(cmd("delete_contact_set", {{QStringLiteral("name"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted contact set %1").arg(node.name);
    }
    break;
  case BrowserNode::Type::Drawing: // mitcad#707
    if (canChange() && m_host.runModelCommand(cmd("delete_drawing", {{QStringLiteral("drawing"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted drawing %1").arg(node.name);
    }
    break;
  case BrowserNode::Type::ImageReference: // mitcad#782
    if (canChange() && m_host.runModelCommand(cmd("delete_image_reference", {{QStringLiteral("name"), node.uid}}))) {
      qDebug().noquote() << QStringLiteral("Deleted image reference %1").arg(node.name);
    }
    break;
  default:
    break;
  }
}

void BrowserController::newComponent(const BrowserNode& node) {
  if (!canChange()) {
    return;
  }
  // Made in the root with modelled bodies: a hybrid assembly (mitcad#171).
  ComponentRequest request;
  request.kind = ComponentRequest::Kind::NewComponent;
  request.parent = node.uid;
  if (askHybridAssembly(m_window, hybridModel(), request).outcome == ComponentAnswer::Outcome::Cancel) {
    return;
  }
  QJsonArray commands;
  if (node.uid != m_snapshot.activeComponent) {
    // It is made in the component the menu was opened on.
    commands.append(cmd("activate_component", {{QStringLiteral("component"), node.uid}}));
  }
  // With the general defaults, everything allowed (mitcad#421).
  commands.append(cmd("create_component"));
  if (m_host.runModelCommands(commands, tr("New Component"))) {
    qDebug().noquote() << QStringLiteral("New component in %1").arg(node.name);
  }
}

HybridModel BrowserController::hybridModel() {
  HybridModel model;
  model.query = [this](const QJsonObject& query) { return m_host.model().queryObject(query); };
  model.run = [this](const QJsonObject& command, QJsonObject* result) {
    return m_host.runModelCommand(command, result);
  };
  return model;
}

// Create Components from Bodies on a body: asked first when the body is
// one of the root's (mitcad#171); moved, it takes the features that made
// it along.
void BrowserController::componentsFromBodies(const BrowserNode& node) {
  if (!canChange()) {
    return;
  }
  ComponentRequest request;
  request.kind = ComponentRequest::Kind::FromBodies;
  request.bodies = QJsonArray{node.uid};
  const ComponentAnswer answer = askHybridAssembly(m_window, hybridModel(), request);
  if (answer.outcome != ComponentAnswer::Outcome::Proceed) {
    return;
  }
  QJsonObject result;
  if (m_host.runModelCommand(cmd("components_from_bodies", {{QStringLiteral("bodies"), QJsonArray{node.uid}}}),
                             &result)) {
    qDebug().noquote() << QStringLiteral("Component from %1").arg(node.name);
  }
}

void BrowserController::moveToComponent(const QJsonObject& selection) {
  if (canChange()) {
    mitcad::moveToComponent(m_window, hybridModel(), selection);
  }
}

// Move to Component… on an occurrence (mitcad#219): the occurrence with
// everything in it under another parent, where it stays in the design; its
// row is selected again at its new path.
void BrowserController::moveOccurrence(const BrowserNode& node) {
  if (node.occurrence.isEmpty() || !canChange()) {
    return;
  }
  const QString moved = mitcad::moveOccurrence(m_window, hybridModel(), node.occurrence);
  if (moved.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Move of %1 cancelled").arg(node.name);
    return;
  }
  SelectionItem item;
  item.kind = SelectKind::Component;
  item.occurrence = moved;
  item.owner = node.uid;
  pick({item}, QStringList());
}

// The features that made a component-from-bodies' bodies go into its
// component, and the step goes (mitcad#171).
void BrowserController::moveHistory(const QString& uid) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr || !canChange()) {
    return;
  }
  const QString name = feature->name;
  QJsonObject result;
  if (m_host.runModelCommand(cmd("move_history_into_component", {{QStringLiteral("feature"), uid}}), &result)) {
    QStringList names;
    for (const QJsonValue& f : result.value(QStringLiteral("features")).toArray()) {
      const DocumentSnapshot::Feature* moved = m_snapshot.feature(f.toString());
      names << (moved != nullptr ? moved->name : f.toString());
    }
    qDebug().noquote() << QStringLiteral("Moved %1 into %2 instead of %3")
                              .arg(names.join(QStringLiteral(", ")),
                                   result.value(QStringLiteral("name")).toString(), name);
  }
}

// Paste and Paste New: the copy is placed where the original is
// and Move/Copy opens on it (P9). OK leaves it where it was moved to, as
// its own placement (no Move in the timeline): paste and placement are one
// undo step. Cancel takes the paste back.
void BrowserController::paste(bool asNew) {
  if (m_clipboard.isEmpty() || !canChange()) {
    return;
  }
  const auto undoDepth = [this] {
    return m_host.model()
        .queryObject({{QStringLiteral("query"), QStringLiteral("document")}})
        .value(QStringLiteral("undo_depth"))
        .toInt();
  };
  const int depth = undoDepth();
  QJsonObject result;
  if (!m_host.runModelCommand(cmd(asNew ? "paste_new" : "copy_occurrence",
                                  {{QStringLiteral("occurrence"), m_clipboard}}),
                              &result)) {
    return;
  }
  const QString uid = result.value(QStringLiteral("occurrence")).toString();
  const QString name = result.value(QStringLiteral("name")).toString();
  const QString label = asNew ? tr("Paste New %1").arg(name) : tr("Paste %1").arg(name);
  qDebug().noquote() << QStringLiteral("Pasted %1%2 as %3")
                            .arg(m_clipboardName, asNew ? QStringLiteral(" (new component)") : QString(), name);
  // The occurrence in the browser's form: its path and its component.
  std::function<const DocumentSnapshot::Occurrence*(const QVector<DocumentSnapshot::Occurrence>&)> find =
      [&](const QVector<DocumentSnapshot::Occurrence>& level) -> const DocumentSnapshot::Occurrence* {
    for (const DocumentSnapshot::Occurrence& placed : level) {
      if (placed.uid == uid) {
        return &placed;
      }
      if (const DocumentSnapshot::Occurrence* inside = find(placed.children)) {
        return inside;
      }
    }
    return nullptr;
  };
  const DocumentSnapshot::Occurrence* pasted = find(m_snapshot.occurrences);
  if (pasted == nullptr) {
    return;
  }
  SelectionItem item;
  item.kind = SelectKind::Component;
  item.occurrence = pasted->path;
  item.owner = pasted->component;
  m_host.startCommand(QStringLiteral("solid.move"), {item}, [this, depth, uid, name, label, undoDepth](bool committed) {
    if (!committed) {
      while (undoDepth() > depth && m_host.runModelCommand(cmd("undo"))) {
      }
      qDebug().noquote() << QStringLiteral("Paste of %1 cancelled").arg(name);
      return;
    }
    // Where Move/Copy put it, then the move feature taken back and the
    // placement set instead.
    QJsonArray transform;
    std::function<void(const QJsonArray&)> look = [&](const QJsonArray& level) {
      for (const QJsonValue& value : level) {
        const QJsonObject placed = value.toObject();
        if (placed.value(QStringLiteral("uid")).toString() == uid) {
          transform = placed.value(QStringLiteral("transform")).toArray();
        }
        look(placed.value(QStringLiteral("children")).toArray());
      }
    };
    look(m_host.model()
             .queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
             .value(QStringLiteral("occurrences"))
             .toArray());
    if (undoDepth() > depth + 1 && !transform.isEmpty() && m_host.runModelCommand(cmd("undo"))) {
      m_host.runModelCommand(cmd("set_occurrence_transform", {{QStringLiteral("occurrence"), uid},
                                                              {QStringLiteral("transform"), transform}}));
    }
    m_host.runModelCommand(cmd("merge_undo", {{QStringLiteral("depth"), depth}, {QStringLiteral("label"), label}}));
    QStringList at;
    for (const QJsonValue& row : transform) {
      at << QString::number(row.toArray().at(3).toDouble(), 'g', 6);
    }
    qDebug().noquote() << QStringLiteral("Placed %1 at (%2)").arg(name, at.join(QStringLiteral(", ")));
  });
}

void BrowserController::changeUnits() {
  if (!canChange()) {
    return;
  }
  bool ok = false;
  qDebug().noquote() << QStringLiteral("Units dialog opened: %1 (%2)")
                            .arg(m_snapshot.lengthUnit, kLengthUnits.join(QStringLiteral(" | ")));
  const QString unit = dialogItem(m_window, tr("Units"), tr("Design length unit:"), kLengthUnits,
                                             std::max(0, static_cast<int>(kLengthUnits.indexOf(m_snapshot.lengthUnit))),
                                             &ok);
  if (ok && unit != m_snapshot.lengthUnit &&
      m_host.runModelCommand(cmd("set_units", {{QStringLiteral("length"), unit}}))) {
    qDebug().noquote() << QStringLiteral("Units: %1").arg(unit);
  }
}

QStringList BrowserController::menuBodies(const BrowserNode& node) const {
  // The selection shown in the browser: a right click on a row that was not
  // selected has selected it alone before the menu opens.
  QStringList bodies;
  for (const SelectionItem& item : m_shownItems) {
    if (item.kind == SelectKind::Body && !item.owner.isEmpty() && !bodies.contains(item.owner)) {
      bodies << item.owner;
    }
  }
  return bodies.contains(node.uid) ? bodies : QStringList{node.uid};
}

Selection BrowserController::menuMoveItems(const BrowserNode& node) const {
  const SelectionItem row = node.item();
  if (!row.isValid()) {
    return {};
  }
  const auto sameRow = [&row](const SelectionItem& shown) {
    return shown.kind == row.kind && shown.owner == row.owner && shown.occurrence == row.occurrence;
  };
  if (std::none_of(m_shownItems.begin(), m_shownItems.end(), sameRow)) {
    return {row};
  }
  Selection items;
  for (const SelectionItem& shown : m_shownItems) {
    if (shown.kind == row.kind) {
      items.append(shown);
    }
  }
  return items;
}

void BrowserController::browserMenu(const BrowserNode& node, const QPoint& at) {
  Menu menu(m_window);
  const SelectionItem item = node.item();
  const bool isolated = !m_host.isolation().isEmpty();
  const auto visibility = [&] {
    if (node.hasEye) {
      menu.add(node.visible ? QStringLiteral("eye-off") : QStringLiteral("eye"),
               node.visible ? tr("Hide") : tr("Show"), [this, node] { toggleVisibility(node); });
    }
  };
  const auto isolate = [&] {
    if (item.isValid()) {
      menu.add(QStringLiteral("isolate"), tr("Isolate"), [this, item] {
        m_host.setIsolation({item});
        qDebug().noquote() << QStringLiteral("Isolated %1").arg(item.describe());
      });
    }
    if (isolated) {
      menu.add(QString(), tr("Unisolate"), [this] {
        m_host.setIsolation({});
        qDebug().noquote() << "Unisolated";
      });
    }
  };
  const auto renameEntry = [&] {
    menu.add(QStringLiteral("edit"), tr("Rename"), [this, node] {
      if (canChange()) {
        m_browser->startRename(node);
      }
    });
  };
  const auto timelineEntry = [&](const QString& feature) {
    menu.add(QStringLiteral("timeline"), tr("Find in Timeline"), [this, feature] { findInTimeline(feature); });
  };
  // Move/Copy on a body's or an occurrence's row (mitcad#735), for the
  // rows selected with it; left out where the command cannot start.
  const auto moveEntry = [&] {
    const Selection moved = menuMoveItems(node);
    if (m_host.offersCommand(QStringLiteral("solid.move"), moved)) {
      menu.add(QStringLiteral("move"), tr("Move/Copy"),
               [this, moved] { m_host.startCommand(QStringLiteral("solid.move"), moved, nullptr); });
    }
  };
  const bool plane = item.kind == SelectKind::Plane;
  // Save As Mesh (mitcad#734): the row's bodies, or those of the body and
  // component rows selected with it, to mesh files; disabled with the
  // reason when none is a solid or mesh body. Last before Collapse All, which
  // ends every row's menu, so that the entries above keep their places.
  const auto saveAsMeshEntry = [&] {
    const SelectionItem own =
        item.isValid() ? item : SelectionItem{SelectKind::Component, node.uid, QString(), QString()}; // the root
    Selection meshItems;
    bool listed = false;
    for (const SelectionItem& shown : m_shownItems) {
      if (shown.kind == SelectKind::Body || shown.kind == SelectKind::Component) {
        meshItems << shown;
        listed = listed || (shown.kind == own.kind && shown.owner == own.owner && shown.occurrence == own.occurrence);
      }
    }
    if (!listed) {
      meshItems = {own};
    }
    const QString blocked = m_host.saveAsMeshBlocked(meshItems);
    if (!blocked.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Save As Mesh disabled: %1").arg(blocked);
    }
    menu.separator();
    menu.add(QStringLiteral("export"), tr("Save As Mesh…"), [this, meshItems] { m_host.saveAsMesh(meshItems); },
             blocked);
  };

  switch (node.type) {
  case BrowserNode::Type::Component: {
    const bool root = node.occurrence.isEmpty();
    if (m_snapshot.activeComponent != node.uid) {
      menu.add(QStringLiteral("radio-on"), tr("Activate Component"), [this, node] { activate(node); });
    }
    menu.add(QStringLiteral("component"), tr("New Component"), [this, node] { newComponent(node); });
    // A drawing of the component, kept in its Drawings folder (mitcad#707).
    menu.add(QStringLiteral("drawing"), tr("Create Drawing"), [this, node] { m_host.createDrawing(node.uid); });
    // Its modelled bodies with their features into a component of their
    // own (mitcad#171).
    for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
      if (c.uid == node.uid && !c.linked && !c.bodies.isEmpty()) {
        menu.add(QStringLiteral("component"), tr("Move Features into a Component"), [this, node] {
          moveToComponent({{QStringLiteral("component"), node.uid},
                           {QStringLiteral("source_occurrence"), node.occurrence}});
        });
      }
    }
    if (!root) {
      menu.add(QStringLiteral("copy"), tr("Copy"), [this, node] {
        m_clipboard = node.occurrence;
        m_clipboardName = node.name;
        qDebug().noquote() << QStringLiteral("Copied %1").arg(node.name);
      });
    }
    if (!m_clipboard.isEmpty()) {
      menu.add(QStringLiteral("paste"), tr("Paste"), [this] { paste(false); });
      menu.add(QStringLiteral("paste"), tr("Paste New"), [this] { paste(true); });
    }
    // Another parent for the occurrence, with what is in it (mitcad#219),
    // named as the body's structure move so that it is not taken for
    // Move/Copy (mitcad#735).
    if (!root) {
      menu.add(QStringLiteral("component"), tr("Move to Component…"), [this, node] { moveOccurrence(node); });
    }
    menu.separator();
    if (!root) {
      moveEntry();
      const DocumentSnapshot::Occurrence* placed = occurrence(node.occurrence);
      const bool grounded = placed != nullptr && placed->grounded;
      menu.add(QStringLiteral("ground"), grounded ? tr("Unground") : tr("Ground"), [this, node, grounded] {
        if (canChange() &&
            m_host.runModelCommand(cmd("ground_occurrence", {{QStringLiteral("occurrence"), node.occurrence},
                                                             {QStringLiteral("grounded"), !grounded}}))) {
          qDebug().noquote() << QStringLiteral("%1 %2").arg(grounded ? QStringLiteral("Unground")
                                                                     : QStringLiteral("Ground"),
                                                            node.name);
        }
      });
      visibility();
      // Shown while a condition holds (mitcad#809).
      menu.add(QStringLiteral("eye"), tr("Show When…"), [this, node] { showOccurrenceWhen(node.occurrence); });
    }
    isolate();
    // The timeline's filter (mitcad#98), this window's view.
    menu.add(QStringLiteral("filter"), tr("Show in Timeline Only"), [this, node] {
      m_timeline->setFilter(node.uid, m_timeline->filterSubcomponents());
    });
    if (!m_timeline->filter().isEmpty()) {
      menu.add(QString(), tr("Show All in Timeline"),
               [this] { m_timeline->setFilter(QString(), m_timeline->filterSubcomponents()); });
    }
    menu.separator();
    renameEntry();
    if (!root) {
      menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    }
    // Last, so that the entries above keep their places (mitcad#220).
    menu.separator();
    menu.add(QStringLiteral("component"), tr("Design Policy…"), [this, node] { openPolicy(node.uid); });
    // The default material of its bodies (mitcad#640, mitcad#655).
    menu.add(QStringLiteral("material"), tr("Material…"), [this, node] { openMaterial({}, node.uid); });
    // The component's and the occurrence's properties (mitcad#496).
    menu.add(QStringLiteral("parameters"), tr("Property Sets…"), [this, node] {
      openPropertySets({QString(), node.occurrence, node.occurrence.isEmpty() ? node.uid : QString()});
    });
    // The options of a library part or a linked design (mitcad#810): its
    // own, shared by its occurrences, so named by the component (Shelf
    // board), not by the occurrence's row (Shelf board:1).
    for (const DocumentSnapshot::Component& c : std::as_const(m_snapshot.components)) {
      if (!root && c.uid == node.uid && (c.library || c.linked)) {
        const QString component = c.uid;
        const QString name = c.name.isEmpty() ? node.name : c.name;
        menu.add(QStringLiteral("parameters"), tr("Edit Options…"),
                 [this, component, name] { editOptions(component, name); });
      }
    }
    // The whole design turned by quarter turns (mitcad#502).
    if (root) {
      menu.add(QStringLiteral("orientation-cube"), tr("Reorient Design…"), [this] { openReorient(); });
    }
    saveAsMeshEntry();
    break;
  }
  case BrowserNode::Type::Body: {
    const QString feature = node.uid.section(QLatin1Char('.'), 0, 0);
    const DocumentSnapshot::Feature* made = m_snapshot.feature(feature);
    menu.add(QStringLiteral("edit"), tr("Edit %1").arg(made != nullptr ? made->name : feature),
             [this, feature] { m_host.editFeature(feature); });
    moveEntry();
    menu.separator();
    visibility();
    // Shown while a condition holds (mitcad#809).
    menu.add(QStringLiteral("eye"), tr("Show When…"), [this, node] { showBodyWhen(node.uid); });
    isolate();
    menu.add(QStringLiteral("component"), tr("Create Components from Bodies"),
             [this, node] { componentsFromBodies(node); });
    // The body with the features that made it (mitcad#171).
    menu.add(QStringLiteral("component"), tr("Move to Component…"), [this, node] {
      moveToComponent({{QStringLiteral("component"), node.component},
                       {QStringLiteral("source_occurrence"), node.occurrence},
                       {QStringLiteral("bodies"), QJsonArray{node.uid}}});
    });
    // For the selected bodies when the row is one of them (mitcad#703).
    const QStringList bodies = menuBodies(node);
    // An fx mark while one of them follows an expression (mitcad#861): the
    // windows' assignment panel shows and edits it.
    bool appearanceBound = false;
    bool materialBound = false;
    for (const QString& uid : bodies) {
      const DocumentSnapshot::Body bodyState = m_snapshot.bodies.value(uid);
      appearanceBound = appearanceBound || !bodyState.appearanceExpr.isEmpty();
      materialBound = materialBound || !bodyState.materialExpr.isEmpty();
    }
    menu.addBound(
        QStringLiteral("appearance"), tr("Appearance…"),
        [this, bodies] {
          Selection targets;
          for (const QString& uid : bodies) {
            targets.append(SelectionItem{SelectKind::Body, uid, QString(), QString()});
          }
          openAppearances(targets);
        },
        appearanceBound);
    // Their material: density, appearance and filament (mitcad#640).
    menu.addBound(QStringLiteral("material"), tr("Material…"), [this, bodies] { openMaterial(bodies); },
                  materialBound);
    // Its properties, its occurrence's and its component's (mitcad#496).
    menu.add(QStringLiteral("parameters"), tr("Property Sets…"),
             [this, node] { openPropertySets({node.uid, node.occurrence, QString()}); });
    // A sheet-metal body (mitcad#210): its rules and Flat Pattern.
    if (const QJsonObject sheet = m_snapshot.sheet(node.uid); !sheet.isEmpty()) {
      const QString rules = sheet.value(QStringLiteral("rules")).toString();
      menu.add(QStringLiteral("sheet-rules"), tr("Edit Sheet Rules"), [this, rules] { m_host.editFeature(rules); });
      if (sheet.value(QStringLiteral("flat_pattern")).isObject()) {
        menu.add(QStringLiteral("export-dxf"), tr("Export Flat Pattern…"),
                 [this, node] { m_host.exportFlatPattern(node.uid); });
      } else {
        menu.add(QStringLiteral("flat-pattern"), tr("Create Flat Pattern"), [this, node] { editFlatPattern(node.uid); });
      }
    }
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    timelineEntry(feature);
    saveAsMeshEntry();
    break;
  }
  case BrowserNode::Type::Sketch:
    menu.add(QStringLiteral("sketch"), tr("Edit Sketch"), [this, node] { m_host.editFeature(node.uid); });
    // Another plane or face, an offset and a rotation on it (mitcad#787).
    menu.add(QStringLiteral("offset-plane"), tr("Redefine Sketch Plane"), [this, node] {
      m_host.startCommand(QStringLiteral("sketch.redefine_plane"),
                          {SelectionItem{SelectKind::Sketch, node.uid, QString(), QString()}}, nullptr);
    });
    menu.add(QStringLiteral("look-at"), tr("Look At"), [this, item] { m_host.lookAt(item); });
    menu.add(QStringLiteral("export"), tr("Save As DXF"), [this, node] { m_host.exportSketch(node.uid); });
    // The timeline's Move to Component (mitcad#218): the sketch from the
    // occurrence of the row, with what goes with it.
    menu.add(QStringLiteral("component"), tr("Move to Component…"), [this, node] {
      moveToComponent({{QStringLiteral("component"), node.component},
                       {QStringLiteral("features"), QJsonArray{node.uid}},
                       {QStringLiteral("source_occurrence"), node.occurrence}});
    });
    menu.separator();
    visibility();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    timelineEntry(node.uid);
    break;
  case BrowserNode::Type::Datum:
    menu.add(QStringLiteral("edit"), tr("Edit %1").arg(node.name), [this, node] { m_host.editFeature(node.uid); });
    if (plane) {
      menu.add(QStringLiteral("sketch"), tr("Create Sketch"), [this, item] { m_host.createSketchOn(item); });
      menu.add(QStringLiteral("look-at"), tr("Look At"), [this, item] { m_host.lookAt(item); });
    }
    menu.separator();
    visibility();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    timelineEntry(node.uid);
    break;
  case BrowserNode::Type::OriginDatum:
    if (plane) {
      menu.add(QStringLiteral("sketch"), tr("Create Sketch"), [this, item] { m_host.createSketchOn(item); });
      menu.add(QStringLiteral("look-at"), tr("Look At"), [this, item] { m_host.lookAt(item); });
    }
    break;
  case BrowserNode::Type::Origin:
  case BrowserNode::Type::Bodies:
  case BrowserNode::Type::Sketches:
  case BrowserNode::Type::Construction: {
    // A Bodies folder's bodies that follow Show When are left as they are
    // (mitcad#863): the entry's tooltip names them; with nothing else in the
    // folder it is disabled, the tooltip saying why.
    QString blocked;
    QString tip;
    if (node.type == BrowserNode::Type::Bodies) {
      const QStringList kept = conditionedBodies(node.component);
      const DocumentSnapshot::Component* def = m_snapshot.component(node.component);
      if (!kept.isEmpty() && def != nullptr && kept.size() == def->bodies.size()) {
        blocked = bodiesFollowConditions(kept);
      } else if (!kept.isEmpty()) {
        tip = leftToConditions(kept);
      }
    }
    const QString entry = node.visible ? tr("Hide All") : tr("Show All");
    const QString icon = node.visible ? QStringLiteral("eye-off") : QStringLiteral("eye");
    if (!blocked.isEmpty()) {
      qDebug().noquote() << QStringLiteral("%1 disabled: %2").arg(entry, blocked);
      menu.add(icon, entry, [this, node] { toggleVisibility(node); }, blocked);
    } else {
      if (!tip.isEmpty()) {
        qDebug().noquote() << QStringLiteral("%1 tooltip: %2").arg(entry, tip);
      }
      menu.addWithTip(icon, entry, [this, node] { toggleVisibility(node); }, tip);
    }
    break;
  }
  case BrowserNode::Type::Units:
  case BrowserNode::Type::Settings:
    menu.add(QStringLiteral("units"), tr("Change Active Units"), [this] { changeUnits(); });
    break;
  case BrowserNode::Type::NamedViews:
    menu.add(QStringLiteral("named-view"), tr("New Named View"), [this] {
      if (canChange()) {
        m_host.saveNamedView(QString(), false);
      }
    });
    break;
  case BrowserNode::Type::NamedView:
    menu.add(QStringLiteral("named-view"), tr("Show View"), [this, node] { m_host.showNamedView(node.uid); });
    if (node.uid == QStringLiteral("home")) {
      menu.add(QStringLiteral("home"), tr("Set Current View as Home"), [this] {
        if (canChange()) {
          m_host.saveNamedView(QStringLiteral("Home"), true);
        }
      });
    } else if (node.uid.startsWith(QStringLiteral("named:"))) {
      menu.add(QStringLiteral("named-view"), tr("Update with Current View"), [this, node] {
        if (canChange()) {
          m_host.saveNamedView(node.uid.mid(6), true);
        }
      });
      menu.separator();
      renameEntry();
      menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    }
    break;
  case BrowserNode::Type::Analyses:
    menu.add(QStringLiteral("section-analysis"), tr("New Section Analysis"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("inspect.section"), {}, nullptr);
      }
    });
    // mitcad#620
    menu.add(QStringLiteral("print-analysis"), tr("New Print Analysis"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("inspect.print_analysis"), {}, nullptr);
      }
    });
    break;
  case BrowserNode::Type::Joints:
    // Joints (mitcad#55): in the component the folder is in, when active.
    for (const auto& [id, icon, text] : {std::tuple{"assemble.joint", "joint", tr("New Joint")},
                                         std::tuple{"assemble.as_built_joint", "as-built-joint", tr("New As-Built Joint")},
                                         std::tuple{"assemble.rigid_group", "rigid-group", tr("New Rigid Group")},
                                         std::tuple{"assemble.constraint", "constraint", tr("New Constraint")}}) {
      const QString command = QString::fromLatin1(id);
      menu.add(QString::fromLatin1(icon), text, [this, command] {
        if (canChange()) {
          m_host.startCommand(command, {}, nullptr);
        }
      });
    }
    break;
  case BrowserNode::Type::Joint: {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(node.uid);
    menu.add(QStringLiteral("edit"), tr("Edit %1").arg(node.name), [this, node] { m_host.editFeature(node.uid); });
    if (!m_snapshot.joint(node.uid).value(QStringLiteral("motions")).toArray().isEmpty()) {
      menu.add(QStringLiteral("drive-joint"), tr("Drive %1").arg(node.name), [this, item] {
        if (canChange()) {
          m_host.startCommand(QStringLiteral("assemble.drive_joint"), {item}, nullptr);
        }
      });
      menu.add(QStringLiteral("drive-joint"), tr("Animate %1").arg(node.name), [this, node] {
        if (canChange()) {
          m_host.animateJoint(node.uid);
        }
      });
    }
    const bool suppressed = feature != nullptr && feature->suppressed;
    menu.add(QStringLiteral("suppress"), suppressed ? tr("Unsuppress") : tr("Suppress"), [this, node, suppressed] {
      if (canChange() &&
          m_host.runModelCommand(cmd("suppress_feature", {{QStringLiteral("uid"), node.uid},
                                                          {QStringLiteral("suppressed"), !suppressed}}))) {
        qDebug().noquote() << QStringLiteral("%1 %2").arg(suppressed ? QStringLiteral("Unsuppressed")
                                                                     : QStringLiteral("Suppressed"),
                                                          node.name);
      }
    });
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    timelineEntry(node.uid);
    break;
  }
  case BrowserNode::Type::Analysis:
    menu.add(QStringLiteral("edit"),
             node.featureType == QStringLiteral("print") ? tr("Edit Print Analysis") : tr("Edit Section Analysis"),
             [this, node] { m_host.editAnalysis(node.uid); });
    menu.separator();
    visibility();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  // Motion links (mitcad#466): in the component the folder is in.
  case BrowserNode::Type::MotionLinks:
    menu.add(QStringLiteral("motion-link"), tr("New Motion Link"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("assemble.motion_link"), {}, nullptr);
      }
    });
    break;
  // Motion studies and contact sets (mitcad#454).
  case BrowserNode::Type::MotionStudies:
    menu.add(QStringLiteral("motion-study"), tr("New Motion Study"), [this] {
      if (canChange()) {
        m_host.openMotionStudy(QString());
      }
    });
    break;
  case BrowserNode::Type::MotionStudy:
    menu.add(QStringLiteral("motion-study"), tr("Play %1").arg(node.name),
             [this, node] { m_host.openMotionStudy(node.uid); });
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  case BrowserNode::Type::ContactSets:
    menu.add(QStringLiteral("contact-set"), tr("New Contact Set"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("assemble.contact_set"), {}, nullptr);
      }
    });
    break;
  case BrowserNode::Type::ContactSet: {
    menu.add(QStringLiteral("edit"), tr("Edit %1").arg(node.name), [this, node] { m_host.editContactSet(node.uid); });
    const bool suppressed = m_snapshot.contactSet(node.uid).value(QStringLiteral("suppressed")).toBool();
    menu.add(QStringLiteral("suppress"), suppressed ? tr("Unsuppress") : tr("Suppress"), [this, node, suppressed] {
      if (canChange() &&
          m_host.runModelCommand(cmd("edit_contact_set", {{QStringLiteral("name"), node.uid},
                                                          {QStringLiteral("suppressed"), !suppressed}}))) {
        qDebug().noquote() << QStringLiteral("%1 %2").arg(suppressed ? QStringLiteral("Unsuppressed")
                                                                     : QStringLiteral("Suppressed"),
                                                          node.name);
      }
    });
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  }
  case BrowserNode::Type::Feature:
    // The timeline's menu of the feature (mitcad#177).
    // (its own menu ends with Collapse All).
    featureMenu(node.uid, at, node);
    return;
  case BrowserNode::Type::SheetRules:
    menu.add(QStringLiteral("sheet-rules"), tr("Edit Sheet Rules"), [this, node] { m_host.editFeature(node.uid); });
    timelineEntry(node.uid);
    break;
  case BrowserNode::Type::FlatPattern:
    menu.add(QStringLiteral("flat-pattern"), tr("Edit Flat Pattern"), [this, node] { editFlatPattern(node.uid); });
    menu.add(node.visible ? QStringLiteral("eye-off") : QStringLiteral("eye"),
             node.visible ? tr("Show Folded") : tr("Show Flat"), [this, node] { toggleVisibility(node); });
    menu.add(QStringLiteral("export-dxf"), tr("Export DXF…"), [this, node] { m_host.exportFlatPattern(node.uid); });
    menu.separator();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  // Annotations (mitcad#702): the window where they are listed and
  // edited, and Measure, which keeps new ones.
  case BrowserNode::Type::Measurements:
    menu.add(QStringLiteral("measure"), tr("Annotations…"), [this] { m_host.openAnnotations(); });
    menu.add(QStringLiteral("measure"), tr("Measure"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("inspect.measure"), {}, nullptr);
      }
    });
    break;
  // Drawings kept in the design (mitcad#707): a new one in the folder's
  // component; a drawing opened over the view.
  case BrowserNode::Type::Drawings:
    menu.add(QStringLiteral("drawing"), tr("Create Drawing"), [this, node] { m_host.createDrawing(node.component); });
    break;
  case BrowserNode::Type::Drawing:
    menu.add(QStringLiteral("drawing"), tr("Open"), [this, node] { m_host.openDrawing(node.uid); });
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  // Image references (mitcad#782): a new one; one edited in its panel or
  // calibrated by two points.
  case BrowserNode::Type::ImageReferences:
    menu.add(QStringLiteral("image-reference"), tr("Insert Image Reference"), [this] {
      if (canChange()) {
        m_host.startCommand(QStringLiteral("insert.image_reference"), {}, nullptr);
      }
    });
    break;
  case BrowserNode::Type::ImageReference:
    menu.add(QStringLiteral("edit"), tr("Edit Image Reference"), [this, node] { m_host.editImageReference(node.uid); });
    menu.add(QStringLiteral("image-calibrate"), tr("Calibrate Image Reference"), [this, node] {
      if (canChange()) {
        m_host.calibrateImageReference(node.uid);
      }
    });
    menu.separator();
    visibility();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    break;
  default:
    break;
  }
  // Every row's menu ends with this presentation action (mitcad#312).
  if (!menu.isEmpty()) {
    menu.separator();
  }
  menu.add(QStringLiteral("folder"), tr("Collapse All"), [this] { m_browser->collapseAll(); });
  menu.exec(at);
}

// ---------------------------------------------------------------------------
// The timeline

void BrowserController::featuresClicked(const QStringList& uids) {
  // The browser shows the features' rows under their sketches too
  // (mitcad#177); a command that takes features takes them instead.
  pick(featureItems(uids), m_host.takesFeatures() ? QStringList() : uids);
}

Selection BrowserController::featureItems(const QStringList& uids) const {
  Selection items;
  const auto add = [&items](const SelectionItem& item) {
    if (item.isValid() && !items.contains(item)) {
      items.append(item);
    }
  };
  if (m_host.takesFeatures()) {
    for (const QString& uid : uids) {
      add({SelectKind::Feature, uid, QString(), QString()});
    }
    return items;
  }
  // Each feature's sketch, datum or bodies are selected. A feature that made
  // no body of its own (a fillet, a chamfer, a hole, a join) is selected as
  // the faces it made on the bodies at the marker (mitcad#28), placed by its
  // component's first occurrence as its sketches are; the main window keeps
  // it only where it finds such faces.
  for (const QString& uid : uids) {
    bool body = false;
    for (const BrowserNode& node : m_browser->nodesOfFeature(uid)) {
      body = body || node.type == BrowserNode::Type::Body;
      add(node.item());
    }
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    if (!body && feature != nullptr) {
      add({SelectKind::Feature, uid, QString(), QString(), m_snapshot.firstOccurrence(feature->component)});
    }
  }
  return items;
}

void BrowserController::pick(const Selection& items, const QStringList& features) {
  // The main window shows the selection here at once (showSelection).
  m_picking = true;
  m_pickedFeatures = features;
  m_host.pickItems(items);
  m_picking = false;
  m_pickedFeatures.clear();
}

void BrowserController::deleteFeature(const QString& uid) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr || !canChange()) {
    return;
  }
  const QString name = feature->name;
  QStringList dependents;
  try {
    for (const QJsonValue& value : m_host.model()
                                       .queryObject({{QStringLiteral("query"), QStringLiteral("dependents")},
                                                     {QStringLiteral("uid"), uid}})
                                       .value(QStringLiteral("dependents"))
                                       .toArray()) {
      dependents << value.toObject().value(QStringLiteral("name")).toString();
    }
  } catch (const std::exception& e) {
    m_host.showStatus(QString::fromUtf8(e.what()), true);
    return;
  }
  if (!dependents.isEmpty()) {
    // The user is asked: the features that use it go too, or nothing does.
    qDebug().noquote() << QStringLiteral("Delete %1: also %2?").arg(name, dependents.join(QStringLiteral(", ")));
    QMessageBox box(QMessageBox::Warning, tr("Delete"), tr("Delete %1?").arg(name), QMessageBox::Cancel,
                    m_window);
    box.setInformativeText(tr("These features use it and will be deleted too:\n%1")
                               .arg(dependents.join(QLatin1Char('\n'))));
    QPushButton* all = box.addButton(tr("Delete &All"), QMessageBox::AcceptRole);
    box.setDefaultButton(all);
    prepareModal(&box);
    execDialog(&box);
    if (box.clickedButton() != all) {
      qDebug().noquote() << QStringLiteral("Delete %1 cancelled").arg(name);
      return;
    }
  }
  QJsonObject result;
  if (m_host.runModelCommand(cmd("delete_feature", {{QStringLiteral("uid"), uid},
                                                    {QStringLiteral("dependents"), !dependents.isEmpty()}}),
                             &result)) {
    qDebug().noquote() << QStringLiteral("Deleted %1%2")
                              .arg(name, dependents.isEmpty()
                                             ? QString()
                                             : QStringLiteral(", ") + dependents.join(QStringLiteral(", ")));
  }
}

void BrowserController::reorder(const QString& uid, int index) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr || feature->index == index || !canChange()) {
    return;
  }
  const QString name = feature->name;
  if (m_host.runModelCommand(cmd("reorder_feature", {{QStringLiteral("uid"), uid}, {QStringLiteral("index"), index}}))) {
    qDebug().noquote() << QStringLiteral("Moved %1 to %2 in the timeline").arg(name).arg(index);
  } else {
    qDebug().noquote() << QStringLiteral("Move of %1 refused: %2").arg(name, m_host.lastError());
  }
}

void BrowserController::pickItems(const Selection& items) { pickRows(items, QStringList()); }

void BrowserController::pickRows(const Selection& items, const QStringList& features) {
  // Picked in the tree itself: showSelection does not scroll it (mitcad#318).
  const bool fromTree = m_fromTree;
  m_fromTree = true;
  pickRowsOfTree(items, features);
  m_fromTree = fromTree;
}

void BrowserController::pickRowsOfTree(const Selection& items, const QStringList& features) {
  // The features of the rows are selected in the timeline (mitcad#177),
  // without its signal: sketches, datums, joints and the features under
  // sketches; a body, a component or the origin's rows clear it.
  QStringList uids = features;
  for (const SelectionItem& item : items) {
    if (item.kind != SelectKind::Body && item.kind != SelectKind::Component &&
        m_snapshot.feature(item.owner) != nullptr && !uids.contains(item.owner)) {
      uids.append(item.owner);
    }
  }
  std::sort(uids.begin(), uids.end(), [this](const QString& a, const QString& b) {
    const DocumentSnapshot::Feature* first = m_snapshot.feature(a);
    const DocumentSnapshot::Feature* second = m_snapshot.feature(b);
    return (first != nullptr ? first->index : -1) < (second != nullptr ? second->index : -1);
  });
  m_timeline->setSelectedFeatures(uids);
  if (features.isEmpty()) {
    pick(items, QStringList());
    return;
  }
  // A feature row selects what the feature's cell in the timeline does.
  Selection all = items;
  for (const SelectionItem& item : featureItems(features)) {
    if (!all.contains(item)) {
      all.append(item);
    }
  }
  pick(all, features);
}

void BrowserController::renameClicked(const BrowserNode& node) {
  // Silently nothing while the model cannot change (a command, a sketch).
  if (m_host.canChangeModel(false)) {
    m_browser->startRename(node);
  }
}

void BrowserController::restoreSearchSelection() {
  // Outside commands and sketches only: a command's inputs keep what the
  // steps gave them.
  if (!m_host.canChangeModel()) {
    return;
  }
  pick(m_searchItems, m_searchFeatures);
  qDebug().noquote() << QStringLiteral("Selection restored after the timeline search: %1 item(s)")
                            .arg(m_searchItems.size());
}

void BrowserController::findInTimeline(const QString& uid) {
  m_timeline->selectFeature(uid);
  qDebug().noquote() << QStringLiteral("Found %1 in the timeline").arg(uid);
}

void BrowserController::editFeature(const QString& uid) { m_host.editFeature(uid); }

void BrowserController::markerDragged(int position) {
  if (m_markerDepth >= 0) {
    m_host.runModelCommand(cmd("set_marker", {{QStringLiteral("position"), position}}));
  }
}

void BrowserController::setMarker(int position) {
  if (position == m_snapshot.marker || !canChange()) {
    return;
  }
  if (m_host.runModelCommand(cmd("set_marker", {{QStringLiteral("position"), position}}))) {
    qDebug().noquote() << QStringLiteral("Marker at %1 of %2").arg(position).arg(m_snapshot.features.size());
  }
}

void BrowserController::markerDragFinished(int position) {
  const int depth = m_markerDepth;
  m_markerDepth = -1;
  if (depth < 0) {
    return;
  }
  if (position == m_markerStart) {
    // Back where it was: no undo step.
    for (int guard = 0; guard < 256 && undoDepth() > depth; ++guard) {
      if (!m_host.runModelCommand(cmd("undo"))) {
        break;
      }
    }
    return;
  }
  if (m_snapshot.marker != position) {
    m_host.runModelCommand(cmd("set_marker", {{QStringLiteral("position"), position}}));
  }
  if (undoDepth() > depth + 1) {
    m_host.runModelCommand(cmd("merge_undo", {{QStringLiteral("depth"), depth},
                                              {QStringLiteral("label"), QStringLiteral("Move Timeline Marker")}}));
  }
  qDebug().noquote() << QStringLiteral("Marker at %1 of %2").arg(position).arg(m_snapshot.features.size());
}

void BrowserController::timelineMenu(const QString& uid, const QPoint& at) { featureMenu(uid, at, BrowserNode()); }

void BrowserController::featureMenu(const QString& uid, const QPoint& at, const BrowserNode& row) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr) {
    return;
  }
  Menu menu(m_window);
  const bool suppressed = feature->suppressed;
  const int index = feature->index;
  menu.add(feature->isSketch() ? QStringLiteral("sketch") : QStringLiteral("edit"),
           feature->isSketch() ? tr("Edit Sketch") : tr("Edit Feature"),
           [this, uid] { m_host.editFeature(uid); });
  // Another plane or face, an offset and a rotation on it (mitcad#787).
  if (feature->isSketch()) {
    menu.add(QStringLiteral("offset-plane"), tr("Redefine Sketch Plane"), [this, uid] {
      m_host.startCommand(QStringLiteral("sketch.redefine_plane"),
                          {SelectionItem{SelectKind::Sketch, uid, QString(), QString()}}, nullptr);
    });
  }
  // Renamed in place where the menu was opened.
  menu.add(QStringLiteral("edit"), tr("Rename"), [this, uid, row] {
    if (!canChange()) {
      return;
    }
    if (row.isValid()) {
      m_browser->startRename(row);
    } else {
      m_timeline->startRename(uid);
    }
  });
  menu.add(QStringLiteral("suppress"), suppressed ? tr("Unsuppress Features") : tr("Suppress Features"),
           [this, uid, suppressed] {
             const DocumentSnapshot::Feature* f = m_snapshot.feature(uid);
             const QString name = f != nullptr ? f->name : uid;
             if (canChange() &&
                 m_host.runModelCommand(cmd("suppress_feature", {{QStringLiteral("uid"), uid},
                                                                 {QStringLiteral("suppressed"), !suppressed}}))) {
               qDebug().noquote() << QStringLiteral("%1 %2").arg(suppressed ? QStringLiteral("Unsuppressed")
                                                                            : QStringLiteral("Suppressed"),
                                                                 name);
             }
           });
  // Suppressed while a condition holds (mitcad#809).
  menu.add(QStringLiteral("suppress"), tr("Suppress When…"), [this, uid] { suppressWhen(uid); });
  menu.add(QStringLiteral("delete"), tr("Delete"), [this, uid] { deleteFeature(uid); });
  // Moving features into components (mitcad#171): a modelled feature with
  // what goes with it, or the history of a component made from bodies.
  if (feature->type == QStringLiteral("component_from_bodies")) {
    menu.add(QStringLiteral("component"), tr("Move the Features That Made It into the Component"),
             [this, uid] { moveHistory(uid); });
  } else if (!feature->isJoint() && feature->type != QStringLiteral("move_occurrence") &&
             feature->type != QStringLiteral("capture_position") &&
             feature->type != QStringLiteral("constraint_set")) {
    const QString component = feature->component;
    menu.add(QStringLiteral("component"), tr("Move to Component…"), [this, uid, component, row] {
      QJsonObject selection{{QStringLiteral("component"), component},
                            {QStringLiteral("features"), QJsonArray{uid}}};
      // A browser row identifies an occurrence. A timeline-only selection
      // has none; the model must establish that its source is unambiguous.
      if (row.isValid()) { selection.insert(QStringLiteral("source_occurrence"), row.occurrence); }
      moveToComponent(selection);
    });
  }
  menu.separator();
  // Timeline groups (P9): the selected run (Shift+click), or this feature.
  QStringList run = m_timeline->selectedFeatures();
  if (!run.contains(uid)) {
    run = QStringList{uid};
  }
  QString group;
  for (const DocumentSnapshot::Group& g : std::as_const(m_snapshot.groups)) {
    if (g.features.contains(uid)) {
      group = g.name;
    }
  }
  if (group.isEmpty()) {
    menu.add(QStringLiteral("folder"), tr("Create Group"), [this, run] { groupFeatures(run); });
  } else {
    menu.add(QStringLiteral("folder"), tr("Collapse Group"),
             [this, group] { m_timeline->setGroupCollapsed(group, true); });
    menu.add(QString(), tr("Ungroup"), [this, group] {
      if (canChange() && m_host.runModelCommand(cmd("ungroup", {{QStringLiteral("name"), group}}))) {
        qDebug().noquote() << QStringLiteral("Ungrouped %1").arg(group);
      }
    });
  }
  menu.add(QStringLiteral("marker"), tr("Roll History Marker Here"), [this, index] { setMarker(index + 1); });
  // Found in the other place.
  if (row.isValid()) {
    menu.add(QStringLiteral("timeline"), tr("Find in Timeline"), [this, uid] { findInTimeline(uid); });
    menu.add(QStringLiteral("folder"), tr("Collapse All"), [this] { m_browser->collapseAll(); });
  } else {
    menu.add(QStringLiteral("browser"), tr("Find in Browser"), [this, uid] {
      const DocumentSnapshot::Feature* f = m_snapshot.feature(uid);
      if (m_browser->reveal(uid)) {
        qDebug().noquote() << QStringLiteral("Found %1 in the browser").arg(f != nullptr ? f->name : uid);
      } else {
        m_host.showStatus(tr("%1 has nothing in the browser.").arg(f != nullptr ? f->name : uid), false);
      }
    });
  }
  menu.exec(at);
}

void BrowserController::groupFeatures(const QStringList& uids) {
  if (!canChange()) {
    return;
  }
  QJsonObject result;
  if (m_host.runModelCommand(cmd("group_features", {{QStringLiteral("features"), QJsonArray::fromStringList(uids)}}),
                             &result)) {
    QStringList names;
    for (const QString& uid : uids) {
      const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
      names << (feature != nullptr ? feature->name : uid);
    }
    qDebug().noquote() << QStringLiteral("Grouped %1 as %2")
                              .arg(names.join(QStringLiteral(", ")), result.value(QStringLiteral("name")).toString());
  }
}

void BrowserController::groupMenu(const QString& name, const QPoint& at) {
  Menu menu(m_window);
  const bool collapsed = m_timeline->isGroupCollapsed(name);
  menu.add(QStringLiteral("folder"), collapsed ? tr("Expand Group") : tr("Collapse Group"),
           [this, name, collapsed] { m_timeline->setGroupCollapsed(name, !collapsed); });
  menu.add(QStringLiteral("edit"), tr("Rename Group"), [this, name] {
    if (!canChange()) {
      return;
    }
    bool ok = false;
    const QString renamed = dialogText(m_window, tr("Rename Group"), tr("Group name:"), name, &ok)
                                .trimmed();
    if (ok && !renamed.isEmpty() && renamed != name &&
        m_host.runModelCommand(cmd("rename_group", {{QStringLiteral("name"), name},
                                                    {QStringLiteral("new_name"), renamed}}))) {
      m_timeline->groupRenamed(name, renamed);
      qDebug().noquote() << QStringLiteral("Renamed group %1 to %2").arg(name, renamed);
    }
  });
  menu.add(QString(), tr("Ungroup"), [this, name] {
    if (canChange() && m_host.runModelCommand(cmd("ungroup", {{QStringLiteral("name"), name}}))) {
      qDebug().noquote() << QStringLiteral("Ungrouped %1").arg(name);
    }
  });
  menu.exec(at);
}

void BrowserController::findInBrowser(const SelectionItem& item) {
  if (m_browser->revealItem(item)) {
    qDebug().noquote() << QStringLiteral("Found %1 in the browser").arg(item.describe());
  } else {
    m_host.showStatus(tr("Nothing in the browser stands for it."), false);
  }
}

QStringList BrowserController::hideableBodies(const Selection& items) {
  QStringList bodies;
  for (const SelectionItem& item : items) {
    const bool onBody = item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
                        item.kind == SelectKind::Vertex || item.kind == SelectKind::Body;
    if (onBody && !item.owner.isEmpty() && !bodies.contains(item.owner)) {
      bodies << item.owner;
    }
  }
  return bodies;
}

QStringList BrowserController::hideableOccurrences(const Selection& items) {
  QStringList occurrences;
  for (const SelectionItem& item : items) {
    const bool onBody = item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
                        item.kind == SelectKind::Vertex || item.kind == SelectKind::Body;
    if (onBody && !item.occurrence.isEmpty() && !occurrences.contains(item.occurrence)) {
      occurrences << item.occurrence;
    }
  }
  return occurrences;
}

void BrowserController::hideBodies(const Selection& items) {
  const QStringList bodies = hideableBodies(items);
  if (bodies.isEmpty()) {
    return;
  }
  // One body is hidden by hand: its Show When condition goes, as the status
  // bar says. Several are a bulk change that leaves those with a condition
  // to it (mitcad#863).
  const bool one = bodies.size() == 1;
  const bool removed = one && !m_snapshot.bodies.value(bodies.first()).visibleWhen.isEmpty();
  QJsonArray commands;
  QStringList names;
  QStringList kept;
  for (const QString& uid : bodies) {
    const QString name = m_host.model().bodyName(uid);
    if (!one && !m_snapshot.bodies.value(uid).visibleWhen.isEmpty()) {
      kept << name;
      continue;
    }
    commands.append(cmd("set_body_visible", {{QStringLiteral("uid"), uid}, {QStringLiteral("visible"), false}}));
    names << name;
  }
  if (!kept.isEmpty()) {
    keptByConditions(tr("Hide Bodies"), QString(), kept);
  }
  if (commands.isEmpty()) {
    return;
  }
  if (m_host.runVisibilityCommands(commands, commands.size() == 1 ? QString() : tr("Hide Bodies"))) {
    qDebug().noquote() << QStringLiteral("Hid body %1").arg(names.join(QStringLiteral(", ")));
    if (removed) {
      conditionRemoved(names.first());
    }
  }
}

void BrowserController::hideOccurrences(const Selection& items) {
  const QStringList occurrences = hideableOccurrences(items);
  if (occurrences.isEmpty()) {
    return;
  }
  // As hideBodies (mitcad#863): one occurrence loses its condition, several
  // leave those with one to it.
  const bool one = occurrences.size() == 1;
  const bool removed = one && !occurrenceCondition(occurrences.first()).isEmpty();
  QJsonArray commands;
  QStringList names;
  QStringList kept;
  for (const QString& path : occurrences) {
    const DocumentSnapshot::Occurrence* found = occurrence(path);
    const QString name = found != nullptr ? found->name : path;
    if (!one && found != nullptr && !found->visibleWhen.isEmpty()) {
      kept << name;
      continue;
    }
    commands.append(
        cmd("set_occurrence_visible", {{QStringLiteral("occurrence"), path}, {QStringLiteral("visible"), false}}));
    names << name;
  }
  if (!kept.isEmpty()) {
    keptByConditions(tr("Hide Components"), QString(), kept);
  }
  if (commands.isEmpty()) {
    return;
  }
  if (m_host.runVisibilityCommands(commands, commands.size() == 1 ? QString() : tr("Hide Components"))) {
    qDebug().noquote() << QStringLiteral("Hid component %1").arg(names.join(QStringLiteral(", ")));
    if (removed) {
      conditionRemoved(names.first());
    }
  }
}

void BrowserController::deleteBodies(const Selection& items) {
  const QStringList bodies = hideableBodies(items);
  if (bodies.isEmpty() || !canChange()) {
    return;
  }
  // A feature works on its own component's bodies: one remove_body in each.
  QStringList components;
  QHash<QString, QJsonArray> byComponent;
  QStringList names;
  for (const QString& uid : bodies) {
    const auto body = m_snapshot.bodies.constFind(uid);
    const QString component = body != m_snapshot.bodies.cend() ? body->component : QString();
    if (!components.contains(component)) {
      components << component;
    }
    byComponent[component].append(uid);
    names << m_host.model().bodyName(uid);
  }
  QJsonArray commands;
  for (const QString& component : components) {
    const QJsonObject def{{QStringLiteral("type"), QStringLiteral("remove_body")},
                          {QStringLiteral("bodies"), byComponent.value(component)}};
    QJsonObject command = cmd("add_feature", {{QStringLiteral("def"), def}});
    if (!component.isEmpty()) {
      command.insert(QStringLiteral("component"), component);
    }
    commands.append(command);
  }
  if (m_host.runModelCommands(commands, bodies.size() == 1 ? tr("Delete %1").arg(names.first()) : tr("Delete Bodies"))) {
    qDebug().noquote() << QStringLiteral("Deleted body %1").arg(names.join(QStringLiteral(", ")));
  }
}

void BrowserController::deleteOccurrences(const Selection& items) {
  const QStringList occurrences = hideableOccurrences(items);
  if (occurrences.isEmpty() || !canChange()) {
    return;
  }
  QJsonArray commands;
  QStringList names;
  for (const QString& path : occurrences) {
    // One inside another selected goes with it.
    if (std::any_of(occurrences.begin(), occurrences.end(), [&path](const QString& other) {
          return path.startsWith(other + QLatin1Char('/'));
        })) {
      continue;
    }
    commands.append(cmd("delete_occurrence", {{QStringLiteral("occurrence"), path}}));
    const DocumentSnapshot::Occurrence* found = occurrence(path);
    names << (found != nullptr ? found->name : path);
  }
  if (m_host.runModelCommands(commands, occurrences.size() == 1 ? tr("Delete %1").arg(names.first())
                                                                : tr("Delete Components"))) {
    qDebug().noquote() << QStringLiteral("Deleted component %1").arg(names.join(QStringLiteral(", ")));
  }
}

bool BrowserController::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_findButton &&
      (event->type() == QEvent::Show || event->type() == QEvent::Resize || event->type() == QEvent::Move)) {
    TestSync::singleShot(0, this, [this] { logFindButton(); });
  }
  return QObject::eventFilter(watched, event);
}

void BrowserController::logFindButton() {
  if (m_findButton == nullptr || !m_findButton->isVisible() || m_window == nullptr) {
    return;
  }
  const QPoint origin = m_window->window()->mapToGlobal(QPoint(0, 0));
  const QPoint global = m_findButton->mapToGlobal(m_findButton->rect().center()) - origin;
  const QString line = QStringLiteral("Find in Design button at %1,%2").arg(global.x()).arg(global.y());
  if (line != m_loggedFindButton) {
    m_loggedFindButton = line;
    qDebug().noquote() << line;
  }
}

void BrowserController::openFind() {
  if (!m_find) {
    m_find = new FindDialog(m_window);
    // The window's buttons act on the model, which rebuilds the rows: queued,
    // and after a job that computes the model.
    const auto later = [this](void (BrowserController::*action)()) {
      return [this, action] {
        if (!m_host.modelBusy()) {
          (this->*action)();
        } else {
          m_host.whenIdle(this, [this, action] { (this->*action)(); });
        }
      };
    };
    connect(m_find, &FindDialog::showRequested, this, later(&BrowserController::findShow), Qt::QueuedConnection);
    connect(m_find, &FindDialog::hideRequested, this, later(&BrowserController::findHide), Qt::QueuedConnection);
    connect(m_find, &FindDialog::appearanceRequested, this, later(&BrowserController::findAppearance),
            Qt::QueuedConnection);
    connect(m_find, &FindDialog::selectRequested, this, later(&BrowserController::findSelect), Qt::QueuedConnection);
    connect(m_find, &FindDialog::selectionPicked, this, later(&BrowserController::findPicked), Qt::QueuedConnection);
    connect(m_find, &FindDialog::revealRequested, this, later(&BrowserController::findReveal), Qt::QueuedConnection);
    // Property values (mitcad#496): the model's search.
    m_find->setPropertySearch([this](const QString& text, QString* error) -> std::optional<QSet<QString>> {
      try {
        const QJsonObject answer =
            m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("property_search")},
                                        {QStringLiteral("text"), text}});
        QSet<QString> owners;
        for (const QJsonValue& match : answer.value(QStringLiteral("matches")).toArray()) {
          owners.insert(match.toObject().value(QStringLiteral("owner")).toString());
        }
        return owners;
      } catch (const std::exception& e) {
        *error = QString::fromUtf8(e.what());
        return std::nullopt;
      }
    });
  }
  m_find->setSnapshot(m_snapshot);
  showDialog(m_find);
  if (!m_find->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_find->raise();
    m_find->activateWindow();
  }
  m_find->focusSearch();
  qDebug().noquote() << "Find in Design opened";
}

Selection BrowserController::findItems(const QVector<FindResult>& results) {
  Selection items;
  for (const FindResult& result : results) {
    SelectionItem item;
    item.kind = result.kind == FindResult::Kind::Body ? SelectKind::Body : SelectKind::Component;
    item.owner = result.uid;
    item.occurrence = result.occurrence;
    items.append(item);
  }
  return items;
}

namespace {
QString countedKinds(int bodies, int components) {
  QStringList parts;
  if (bodies > 0) {
    parts << (bodies == 1 ? QStringLiteral("1 body") : QStringLiteral("%1 bodies").arg(bodies));
  }
  if (components > 0) {
    parts << (components == 1 ? QStringLiteral("1 component") : QStringLiteral("%1 components").arg(components));
  }
  return parts.join(QStringLiteral(", "));
}
} // namespace

void BrowserController::findVisibility(bool show) {
  if (!m_find) {
    return;
  }
  // The bodies by uid and the components by occurrence, each once: the same
  // commands as the light bulbs, in one undo step. Those with a Show When
  // condition follow it (mitcad#863): Show and Hide of the found items are
  // a bulk change.
  QStringList bodies;
  QStringList occurrences;
  QStringList seen;
  QStringList kept;
  QJsonArray commands;
  for (const FindResult& result : m_find->targets()) {
    if (result.kind == FindResult::Kind::Body) {
      if (!seen.contains(result.uid)) {
        seen << result.uid;
        const DocumentSnapshot::Body body = m_snapshot.bodies.value(result.uid);
        if (!body.visibleWhen.isEmpty()) {
          kept << (body.name.isEmpty() ? result.uid : body.name);
          continue;
        }
        bodies << result.uid;
        commands.append(
            cmd("set_body_visible", {{QStringLiteral("uid"), result.uid}, {QStringLiteral("visible"), show}}));
      }
    } else if (!seen.contains(result.occurrence)) {
      seen << result.occurrence;
      const DocumentSnapshot::Occurrence* found = occurrence(result.occurrence);
      if (found != nullptr && !found->visibleWhen.isEmpty()) {
        kept << found->name;
        continue;
      }
      occurrences << result.occurrence;
      commands.append(cmd("set_occurrence_visible",
                          {{QStringLiteral("occurrence"), result.occurrence}, {QStringLiteral("visible"), show}}));
    }
  }
  if ((commands.isEmpty() && kept.isEmpty()) || !canChange(true)) {
    return;
  }
  if (!kept.isEmpty()) {
    keptByConditions(show ? tr("Show Found Items") : tr("Hide Found Items"), QString(), kept);
  }
  if (commands.isEmpty()) {
    return;
  }
  const QString counts = countedKinds(static_cast<int>(bodies.size()), static_cast<int>(occurrences.size()));
  if (m_host.runVisibilityCommands(commands, show ? tr("Show Found Items") : tr("Hide Found Items"))) {
    qDebug().noquote() << QStringLiteral("Find in Design: %1 %2").arg(show ? QStringLiteral("showed") : QStringLiteral("hid"), counts);
  }
}

void BrowserController::findShow() { findVisibility(true); }

void BrowserController::findHide() { findVisibility(false); }

void BrowserController::findAppearance() {
  if (!m_find) {
    return;
  }
  // The bodies of the results; of a component, the bodies of its definition.
  Selection targets;
  QStringList uids;
  const auto add = [&targets, &uids](const QString& uid) {
    if (!uids.contains(uid)) {
      uids << uid;
      targets.append(SelectionItem{SelectKind::Body, uid, QString(), QString()});
    }
  };
  for (const FindResult& result : m_find->targets()) {
    if (result.kind == FindResult::Kind::Body) {
      add(result.uid);
    } else if (const DocumentSnapshot::Component* component = m_snapshot.component(result.component)) {
      for (const QString& uid : component->bodies) {
        add(uid);
      }
    }
  }
  if (targets.isEmpty()) {
    m_host.showStatus(tr("The results have no bodies to give an appearance."), false);
    return;
  }
  qDebug().noquote() << QStringLiteral("Find in Design: appearance for %1").arg(countedKinds(static_cast<int>(uids.size()), 0));
  openAppearances(targets);
}

void BrowserController::findSelect() {
  if (!m_find || !m_host.canChangeModel()) {
    return;
  }
  const QVector<FindResult> targets = m_find->targets();
  int bodies = 0;
  for (const FindResult& result : targets) {
    bodies += result.kind == FindResult::Kind::Body ? 1 : 0;
  }
  pick(findItems(targets), QStringList());
  qDebug().noquote() << QStringLiteral("Find in Design: selected %1")
                            .arg(countedKinds(bodies, static_cast<int>(targets.size()) - bodies));
}

void BrowserController::findPicked() {
  // Selecting rows selects them in the view and the browser; not while a
  // command or a sketch takes the picks.
  if (!m_find || !m_host.canChangeModel()) {
    return;
  }
  pick(findItems(m_find->selected()), QStringList());
}

void BrowserController::findReveal() {
  FindResult result;
  if (m_find && m_find->current(&result)) {
    findInBrowser(findItems({result}).first());
  }
}

void BrowserController::openParameters() {
  if (!m_parameters) {
    m_parameters = new ParametersDialog(m_host, m_window);
  }
  showDialog(m_parameters);
  if (!m_parameters->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_parameters->raise();
    m_parameters->activateWindow();
  }
  qDebug().noquote() << "Parameters dialog opened";
  m_parameters->refresh();
}

ParametersDialog* BrowserController::parametersDialog() const { return m_parameters.data(); }

void BrowserController::openConfigurations() {
  if (!m_configurations) {
    m_configurations = new ConfigurationsDialog(m_host, m_window);
    // The Bindings tab's Edit… and Select in Browser (mitcad#862).
    BindingActions actions;
    actions.edit = [this](const BindingEntry& entry) { editBinding(entry); };
    actions.select = [this](const BindingEntry& entry) { selectBinding(entry); };
    m_configurations->setBindingActions(actions);
  }
  showDialog(m_configurations);
  if (!m_configurations->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_configurations->raise();
    m_configurations->activateWindow();
  }
  qDebug().noquote() << "Configurations window opened";
  m_configurations->refresh();
}

ConfigurationsDialog* BrowserController::configurationsDialog() const { return m_configurations.data(); }

// Bindings at a glance (mitcad#862).

void BrowserController::browserBindings(const BrowserNode& node, const QRect& anchor, bool pinned) {
  BindingItem item;
  switch (node.type) {
  case BrowserNode::Type::Body:
    item.kind = BindingItem::Kind::Body;
    item.uid = node.uid;
    break;
  case BrowserNode::Type::Feature:
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
    item.kind = BindingItem::Kind::Feature;
    item.uid = node.uid;
    break;
  case BrowserNode::Type::Component:
    if (!node.occurrence.isEmpty()) {
      item.kind = BindingItem::Kind::Occurrence;
      item.uid = node.occurrence.section(QLatin1Char('/'), -1);
      item.path = node.occurrence;
    }
    break;
  default:
    break;
  }
  showBindingsCard(item, anchor, pinned);
}

void BrowserController::timelineBindings(const QString& uid, const QRect& anchor, bool pinned) {
  showBindingsCard({BindingItem::Kind::Feature, uid, QString()}, anchor, pinned);
}

void BrowserController::showBindingsCard(const BindingItem& item, const QRect& anchor, bool pinned) {
  if (!item.isValid()) {
    return;
  }
  if (!m_bindingsCard) {
    BindingsCard::Actions actions;
    actions.edit = [this](const BindingEntry& entry) { editBinding(entry); };
    actions.showAll = [this] { showAllBindings(); };
    m_bindingsCard = new BindingsCard(m_host.model(), actions, m_window);
  }
  m_bindingsCard->showFor(item, anchor, pinned);
}

void BrowserController::editBinding(const BindingEntry& entry) {
  switch (entry.kind) {
  case BindingEntry::Kind::Appearance:
    openAppearances({SelectionItem{SelectKind::Body, entry.uid, QString(), QString()}});
    break;
  case BindingEntry::Kind::FaceAppearance:
    openAppearances({SelectionItem{SelectKind::Face, entry.uid, entry.face, QString()}});
    break;
  case BindingEntry::Kind::Material:
    openMaterial({entry.uid});
    break;
  case BindingEntry::Kind::Shown:
    showBodyWhen(entry.uid);
    break;
  case BindingEntry::Kind::Suppressed:
    suppressWhen(entry.uid);
    break;
  case BindingEntry::Kind::OccurrenceShown:
    showOccurrenceWhen(entry.path);
    break;
  }
}

void BrowserController::selectBinding(const BindingEntry& entry) {
  switch (entry.kind) {
  case BindingEntry::Kind::Suppressed:
    reveal(entry.uid);
    return;
  case BindingEntry::Kind::OccurrenceShown: {
    const DocumentSnapshot::Occurrence* placed = occurrence(entry.path);
    if (placed != nullptr) {
      SelectionItem item{SelectKind::Component, placed->component, QString(), QString()};
      item.occurrence = entry.path;
      findInBrowser(item);
    }
    return;
  }
  case BindingEntry::Kind::Appearance:
  case BindingEntry::Kind::Material:
  case BindingEntry::Kind::Shown:
  case BindingEntry::Kind::FaceAppearance: {
    // A body of a component: its first placement's row.
    SelectionItem item{SelectKind::Body, entry.uid, QString(), QString()};
    item.occurrence = m_snapshot.firstOccurrence(m_snapshot.bodies.value(entry.uid).component);
    findInBrowser(item);
    return;
  }
  }
}

void BrowserController::showAllBindings() {
  openConfigurations();
  if (m_configurations) {
    m_configurations->showBindings();
  }
}

void BrowserController::openAppearances(const Selection& targets) {
  if (!m_appearances) {
    m_appearances = new AppearanceDialog(m_host, m_window);
    // Add Option… of Varies with (mitcad#861).
    m_appearances->openConfigurations = [this] { openConfigurations(); };
  }
  showDialog(m_appearances);
  if (!m_appearances->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_appearances->raise();
    m_appearances->activateWindow();
  }
  qDebug().noquote() << "Appearances dialog opened";
  m_appearances->refresh();
  m_appearances->setTargets(targets);
}

void BrowserController::openPropertySets(const PropertyTarget& target) {
  if (!m_propertySets) {
    m_propertySets = new PropertySetsDialog(m_host, m_window);
    // Material… of the print settings' Filament row (mitcad#655).
    m_propertySets->openMaterial = [this](const QStringList& bodies, const QString& component) {
      openMaterial(bodies, component);
    };
    // Edit… beside a Printer or Process profile.
    m_propertySets->openProfile = [this](const QString& kind, const QString& key) { openProfiles(kind, key); };
    m_propertySets->openOwnSettings = [this](const QString& owner, const QString& name) {
      openOwnSettings(owner, name);
    };
  }
  showDialog(m_propertySets);
  if (!m_propertySets->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_propertySets->raise();
    m_propertySets->activateWindow();
  }
  QStringList what;
  for (const QString& part : {target.body, target.occurrence, target.component}) {
    if (!part.isEmpty()) {
      what << part;
    }
  }
  qDebug().noquote() << QStringLiteral("Property Sets opened for %1").arg(what.join(QStringLiteral(", ")));
  m_propertySets->setTarget(target);
}

void BrowserController::openProfiles(const QString& kind, const QString& key) {
  if (!m_profiles) {
    m_profiles = new ProfilesDialog(m_host, m_window);
    m_profiles->openMyLibrary = [this] { openMyLibrary(); };
  }
  showDialog(m_profiles);
  if (!m_profiles->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_profiles->raise();
    m_profiles->activateWindow();
  }
  qDebug().noquote() << QStringLiteral("Printers and processes opened");
  m_profiles->showItem(kind, key);
}

void BrowserController::openMyLibrary() {
  if (!m_myLibrary) {
    m_myLibrary = new MyLibraryDialog(m_window);
    // The design's windows read the libraries again (mitcad#704); a
    // changed item the design uses is asked about there, as another
    // design's change.
    connect(m_myLibrary, &MyLibraryDialog::libraryChanged, this, [this] {
      if (m_materials && m_materials->isVisible()) {
        m_materials->refresh();
      }
      if (m_profiles && m_profiles->isVisible()) {
        m_profiles->refresh();
      }
      m_host.commandInputsChanged();
    });
  }
  m_myLibrary->setProjectFolder(m_host.projectFolder());
  m_myLibrary->refresh();
  showDialog(m_myLibrary);
  if (!m_myLibrary->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_myLibrary->raise();
    m_myLibrary->activateWindow();
  }
  qDebug().noquote() << QStringLiteral("My Library opened%1")
                            .arg(m_host.projectFolder().isEmpty() ? QString() : QStringLiteral(" in a project"));
}

void BrowserController::openOwnSettings(const QString& owner, const QString& name) {
  if (!m_ownSettings) {
    m_ownSettings = new OwnSettingsDialog(m_host, m_window);
  }
  showDialog(m_ownSettings);
  if (!m_ownSettings->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_ownSettings->raise();
    m_ownSettings->activateWindow();
  }
  m_ownSettings->setOwner(owner, name);
}

void BrowserController::openPurgeVolumes(const QStringList& bodies) {
  if (!m_purgeVolumes) {
    m_purgeVolumes = new PurgeVolumesDialog(m_host, m_window);
  }
  showDialog(m_purgeVolumes);
  if (!m_purgeVolumes->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_purgeVolumes->raise();
    m_purgeVolumes->activateWindow();
  }
  m_purgeVolumes->setBodies(bodies);
}

void BrowserController::openPolicy(const QString& component) {
  if (!canChange()) {
    return;
  }
  PolicyDialog dialog([this](const QJsonObject& query) { return m_host.model().queryObject(query); }, component,
                      m_window);
  prepareModal(&dialog);
  qDebug().noquote() << QStringLiteral("Design Policy dialog opened for %1").arg(m_snapshot.componentName(component));
  if (execDialog(&dialog) != QDialog::Accepted || !dialog.changed()) {
    qDebug() << "Design Policy dialog closed without a change";
    return;
  }
  QJsonObject result;
  if (m_host.runModelCommand(cmd("set_policy", {{QStringLiteral("component"), component},
                                                {QStringLiteral("policy"), dialog.policy()}}),
                             &result)) {
    const QJsonObject policy = result.value(QStringLiteral("policy")).toObject();
    const QString preset = policy.value(QStringLiteral("preset")).toString();
    qDebug().noquote() << QStringLiteral("Policy of %1 set: %2")
                              .arg(m_snapshot.componentName(component),
                                   preset.isEmpty() ? QStringLiteral("custom") : preset);
  }
}

void BrowserController::editOptions(const QString& component, const QString& name) {
  if (!canChange()) {
    return;
  }
  // A library part's table at its version (mitcad#949): its rows'
  // standards and suppliers' articles for the filters above the choosers,
  // with the suppliers ({id, name, url}) and the component's
  // `supplier_url` for the table of the row's articles (mitcad#979).
  QJsonObject libraryTable;
  try {
    const QJsonArray parts =
        m_host.model().queryObject({{QStringLiteral("query"), QStringLiteral("library_parts")}}).value(
            QStringLiteral("parts")).toArray();
    for (const QJsonValue& value : parts) {
      const QJsonObject part = value.toObject();
      const QJsonObject library = part.value(QStringLiteral("library")).toObject();
      if (part.value(QStringLiteral("component")).toString() != component || library.isEmpty()) {
        continue;
      }
      const QJsonObject shown = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_show")},
                                                {QStringLiteral("url"), library.value(QStringLiteral("url"))},
                                                {QStringLiteral("id"), library.value(QStringLiteral("library"))},
                                                {QStringLiteral("rev"), library.value(QStringLiteral("rev"))}});
      for (const QJsonValue& shownComponent : shown.value(QStringLiteral("components")).toArray()) {
        const QJsonObject entry = shownComponent.toObject();
        if (entry.value(QStringLiteral("id")).toString() == library.value(QStringLiteral("component")).toString()) {
          libraryTable = entry.value(QStringLiteral("configurations")).toObject();
          libraryTable.insert(QStringLiteral("standards"), entry.value(QStringLiteral("standards")));
          // The suppliers' sites and the component's product page for the
          // table of the row's articles (mitcad#979).
          QJsonArray suppliers = shown.value(QStringLiteral("manifest"))
                                     .toObject()
                                     .value(QStringLiteral("suppliers"))
                                     .toArray();
          if (suppliers.isEmpty()) {
            suppliers = entry.value(QStringLiteral("suppliers")).toArray();
          }
          libraryTable.insert(QStringLiteral("suppliers"), suppliers);
          const QString page = entry.value(QStringLiteral("supplier_url")).toString();
          libraryTable.insert(QStringLiteral("supplier_url"),
                              page.isEmpty() ? library.value(QStringLiteral("supplier_url")).toString() : page);
        }
      }
    }
  } catch (const std::exception&) {
    // Without the library: no filters.
  }
  EditOptionsDialog dialog(
      [this](const QJsonObject& query) {
        try {
          return m_host.model().queryObject(query);
        } catch (const std::exception& error) {
          return QJsonObject{{QStringLiteral("error"),
                              QJsonObject{{QStringLiteral("message"), QString::fromUtf8(error.what())}}}};
        }
      },
      component, name, libraryTable, m_window);
  prepareModal(&dialog);
  qDebug().noquote() << QStringLiteral("Edit Options opened for %1").arg(name);
  if (execDialog(&dialog) != QDialog::Accepted || !dialog.hasOptions() || !canChange()) {
    qDebug() << "Edit Options closed without a change";
    return;
  }
  if (m_host.runModelCommand(setComponentParametersCommand(component, dialog.parameters()))) {
    qDebug().noquote() << QStringLiteral("Edit Options of %1: %2").arg(name, dialog.summary());
  }
}

void BrowserController::openMaterial(const QStringList& bodies, const QString& component, bool keepOpen) {
  if (!m_materials) {
    m_materials = new MaterialsDialog(m_host, m_window);
    m_materials->editAppearances = [this] { openAppearances({}); };
    m_materials->openMyLibrary = [this] { openMyLibrary(); };
    m_materials->openConfigurations = [this] { openConfigurations(); };
  }
  // The targets by name: Assign to Body1, Assign to Arm.
  QStringList names;
  if (!component.isEmpty()) {
    names << m_snapshot.componentName(component);
  } else {
    try {
      for (const QJsonValue& value : m_host.model().queryArray(QStringLiteral("bodies"))) {
        const QJsonObject row = value.toObject();
        if (bodies.contains(row.value(QStringLiteral("uid")).toString())) {
          names << row.value(QStringLiteral("name")).toString();
        }
      }
    } catch (const std::exception&) {
      // Named by their count.
    }
  }
  showDialog(m_materials);
  if (!m_materials->testAttribute(Qt::WA_DontShowOnScreen)) {
    m_materials->raise();
    m_materials->activateWindow();
  }
  const QString what = !component.isEmpty() ? QStringLiteral("component %1").arg(component)
                                             : QStringLiteral("bodies %1").arg(bodies.join(QStringLiteral(", ")));
  qDebug().noquote() << (component.isEmpty() && bodies.isEmpty() ? QStringLiteral("Materials opened")
                                                                 : QStringLiteral("Materials opened for %1").arg(what));
  // From 3D Print's Assign materials… the window stays after Assign
  // (mitcad#655): several new materials, several bodies.
  m_materials->setKeepOpen(keepOpen);
  m_materials->setTargets(bodies, component, names);
}

void BrowserController::openReorient() {
  if (!canChange()) {
    return;
  }
  ReorientDialog dialog([this](const QJsonObject& command,
                               QJsonObject* result) { return m_host.runModelCommand(command, result); },
                        [this] { return m_host.lastError(); }, m_window);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return;
  }
  const QJsonObject result = dialog.result();
  qDebug().noquote() << QStringLiteral("Reorient Design: %1").arg(result.value(QStringLiteral("turn")).toString());
  // Where each body is now, for the log (UI tests read it).
  const auto number = [](double v) {
    const double rounded = std::round(v * 1e6) / 1e6;
    return QString::number(rounded == 0.0 ? 0.0 : rounded, 'g', 12);
  };
  const auto point = [&number](const QJsonValue& value) {
    const QJsonArray p = value.toArray();
    return QStringLiteral("(%1, %2, %3)").arg(number(p.at(0).toDouble()), number(p.at(1).toDouble()),
                                              number(p.at(2).toDouble()));
  };
  QJsonArray instances;
  try {
    instances = m_host.model()
                    .query({{QStringLiteral("query"), QStringLiteral("instances")},
                            {QStringLiteral("properties"), true}})
                    .toArray();
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Reorient Design: %1").arg(QString::fromUtf8(e.what()));
  }
  for (const QJsonValue& value : instances) {
    const QJsonObject instance = value.toObject();
    const QJsonObject box = instance.value(QStringLiteral("bbox")).toObject();
    if (box.isEmpty()) {
      continue;
    }
    qDebug().noquote() << QStringLiteral("Reorient Design body %1: box %2 - %3")
                              .arg(instance.value(QStringLiteral("name")).toString(),
                                   point(box.value(QStringLiteral("min"))), point(box.value(QStringLiteral("max"))));
  }
}

// Bindings (mitcad#809).

void BrowserController::suppressWhen(const QString& feature) {
  const DocumentSnapshot::Feature* f = m_snapshot.feature(feature);
  if (f == nullptr || !canChange()) {
    return;
  }
  const QString name = f->name;
  const QString current = f->suppressedWhen;
  ConditionDialog dialog(tr("Suppress %1 When").arg(name),
                         tr("%1 is suppressed while the condition holds (gives a number that is not 0), "
                            "as Color = 'Black' or Length > 1 m. The features after it recompute when it "
                            "changes.")
                             .arg(name),
                         current, m_window);
  if (execDialog(&dialog) != QDialog::Accepted || dialog.condition() == current) {
    return;
  }
  const QString condition = dialog.condition();
  if (m_host.runModelCommand(cmd("suppress_feature", {{QStringLiteral("uid"), feature},
                                                      {QStringLiteral("expression"), condition}}))) {
    qDebug().noquote() << (condition.isEmpty() ? QStringLiteral("Suppress When of %1 removed").arg(name)
                                               : QStringLiteral("Suppress %1 when %2").arg(name, condition));
  } else {
    qDebug().noquote() << QStringLiteral("Suppress When of %1 refused: %2").arg(name, m_host.lastError());
  }
}

void BrowserController::showBodyWhen(const QString& body) {
  if (!canChange()) {
    return;
  }
  const DocumentSnapshot::Body found = m_snapshot.bodies.value(body);
  const QString name = found.name.isEmpty() ? body : found.name;
  const QString current = found.visibleWhen;
  ConditionDialog dialog(tr("Show %1 When").arg(name),
                         tr("%1 is shown while the condition holds (gives a number that is not 0), "
                            "as Color = 'Black' or Length > 1 m.")
                             .arg(name),
                         current, m_window);
  if (execDialog(&dialog) != QDialog::Accepted || dialog.condition() == current) {
    return;
  }
  const QString condition = dialog.condition();
  if (m_host.runModelCommand(cmd("set_body_visible", {{QStringLiteral("uid"), body},
                                                      {QStringLiteral("expression"), condition}}))) {
    qDebug().noquote() << (condition.isEmpty() ? QStringLiteral("Show When of %1 removed").arg(name)
                                               : QStringLiteral("Show %1 when %2").arg(name, condition));
  } else {
    qDebug().noquote() << QStringLiteral("Show When of %1 refused: %2").arg(name, m_host.lastError());
  }
}

void BrowserController::showOccurrenceWhen(const QString& path) {
  const DocumentSnapshot::Occurrence* found = occurrence(path);
  if (found == nullptr || !canChange()) {
    return;
  }
  const QString uid = found->uid;
  const QString name = found->name;
  const QString current = found->visibleWhen;
  ConditionDialog dialog(tr("Show %1 When").arg(name),
                         tr("%1 and what it contains are shown while the condition holds (gives a number "
                            "that is not 0), as Color = 'Black' or Length > 1 m.")
                             .arg(name),
                         current, m_window);
  if (execDialog(&dialog) != QDialog::Accepted || dialog.condition() == current) {
    return;
  }
  const QString condition = dialog.condition();
  if (m_host.runModelCommand(cmd("set_occurrence_visible", {{QStringLiteral("occurrence"), uid},
                                                            {QStringLiteral("expression"), condition}}))) {
    qDebug().noquote() << (condition.isEmpty() ? QStringLiteral("Show When of %1 removed").arg(name)
                                               : QStringLiteral("Show %1 when %2").arg(name, condition));
  } else {
    qDebug().noquote() << QStringLiteral("Show When of %1 refused: %2").arg(name, m_host.lastError());
  }
}

} // namespace mitcad
