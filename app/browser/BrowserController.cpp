// SPDX-License-Identifier: MIT
#include "BrowserController.hpp"

#include <functional>
#include <utility>

#include <QInputDialog>
#include <QJsonArray>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QPushButton>
#include <QToolButton>
#include <QWidget>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "DocumentHost.hpp"
#include "ParametersDialog.hpp"
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
  void separator() { m_menu.addSeparator(); }
  bool isEmpty() const { return m_entries.isEmpty(); }
  void exec(const QPoint& at) {
    qDebug().noquote() << QStringLiteral("Context menu: %1").arg(m_entries.join(QStringLiteral(" | ")));
    m_menu.exec(at);
  }

private:
  QMenu m_menu;
  QStringList m_entries;
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
  m_timeline = new TimelineWidget(host.model(), window);
  m_failures = new QToolButton(window);
  m_failures->setAutoRaise(true);
  m_failures->setFocusPolicy(Qt::NoFocus);
  m_failures->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  m_failures->setIcon(themeIcon(QStringLiteral("warning")));
  setErrorStyleSheet(m_failures, QStringLiteral("QToolButton { color: %1; }"));
  m_failures->hide();

  follow(m_browser, &BrowserPanel::itemsPicked, &BrowserController::pickItems);
  follow(m_browser, &BrowserPanel::eyeClicked, &BrowserController::toggleVisibility);
  follow(m_browser, &BrowserPanel::activateClicked, &BrowserController::activate);
  follow(m_browser, &BrowserPanel::renamed, &BrowserController::rename);
  follow(m_browser, &BrowserPanel::doubleClicked, &BrowserController::browserDoubleClicked);
  follow(m_browser, &BrowserPanel::deleteRequested, &BrowserController::deleteNode);
  follow(m_browser, &BrowserPanel::contextMenuRequested, &BrowserController::browserMenu, true);

  follow(m_timeline, &TimelineWidget::featuresClicked, &BrowserController::featuresClicked);
  follow(m_timeline, &TimelineWidget::editRequested, &BrowserController::editFeature);
  follow(m_timeline, &TimelineWidget::contextMenuRequested, &BrowserController::timelineMenu, true);
  follow(m_timeline, &TimelineWidget::groupMenuRequested, &BrowserController::groupMenu, true);
  follow(m_timeline, &TimelineWidget::deleteRequested, &BrowserController::deleteFeature);
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

  connect(m_failures, &QToolButton::clicked, this, [this] {
    const QVector<const DocumentSnapshot::Feature*> failed = m_snapshot.failures();
    if (failed.size() == 1) {
      reveal(failed.first()->uid);
      return;
    }
    Menu menu(m_window);
    for (const DocumentSnapshot::Feature* feature : failed) {
      const QString uid = feature->uid;
      menu.add(featureIcon(feature->type), QStringLiteral("%1: %2").arg(feature->name, feature->error),
               [this, uid] { reveal(uid); });
    }
    menu.exec(m_failures->mapToGlobal(QPoint(0, 0)));
  });
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
}

void BrowserController::showSelection(const Selection& items) { m_browser->showSelection(items); }

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
}

void BrowserController::reveal(const QString& uid) {
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
  if (feature == nullptr) {
    return;
  }
  m_timeline->selectFeature(uid);
  m_browser->reveal(uid);
  if (!feature->error.isEmpty()) {
    m_host.showStatus(QStringLiteral("%1: %2").arg(feature->name, feature->error), true);
  }
  qDebug().noquote() << QStringLiteral("Revealed %1").arg(feature->name);
}

bool BrowserController::canChange(bool visibilityOnly) {
  if (m_host.canChangeModel(visibilityOnly)) {
    return true;
  }
  m_host.showStatus(tr("Finish the command or the sketch first."), true);
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

void BrowserController::toggleVisibility(const BrowserNode& node) {
  const bool show = !node.visible;
  const QString verb = show ? QStringLiteral("shown") : QStringLiteral("hidden");
  if (node.type == BrowserNode::Type::Origin) {
    m_host.setOriginShown(show);
    qDebug().noquote() << QStringLiteral("Visibility %1: %2").arg(node.path, verb);
    return;
  }
  if (!canChange(true)) {
    return;
  }
  bool done = false;
  switch (node.type) {
  case BrowserNode::Type::Body:
    done = m_host.runModelCommand(cmd("set_body_visible", {{QStringLiteral("uid"), node.uid},
                                                           {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
    done = m_host.runModelCommand(cmd("set_feature_visible", {{QStringLiteral("uid"), node.uid},
                                                              {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Component:
    done = m_host.runModelCommand(cmd("set_occurrence_visible",
                                      {{QStringLiteral("occurrence"), node.occurrence},
                                       {QStringLiteral("visible"), show}}));
    break;
  case BrowserNode::Type::Bodies: {
    QJsonArray commands;
    const DocumentSnapshot::Component* component = m_snapshot.component(node.component);
    for (const QString& uid : component != nullptr ? component->bodies : QStringList()) {
      commands.append(cmd("set_body_visible", {{QStringLiteral("uid"), uid}, {QStringLiteral("visible"), show}}));
    }
    done = m_host.runModelCommands(commands, show ? tr("Show Bodies") : tr("Hide Bodies"));
    break;
  }
  case BrowserNode::Type::Sketches:
  case BrowserNode::Type::Construction: {
    const bool sketches = node.type == BrowserNode::Type::Sketches;
    QJsonArray commands;
    for (const DocumentSnapshot::Feature& feature : m_snapshot.features) {
      if (feature.component == node.component && feature.isActive() &&
          (sketches ? feature.isSketch() : feature.isConstruction())) {
        commands.append(cmd("set_feature_visible", {{QStringLiteral("uid"), feature.uid},
                                                    {QStringLiteral("visible"), show}}));
      }
    }
    const QString what = sketches ? tr("Sketches") : tr("Construction");
    done = m_host.runModelCommands(commands, (show ? tr("Show %1") : tr("Hide %1")).arg(what));
    break;
  }
  default:
    return;
  }
  if (done) {
    qDebug().noquote() << QStringLiteral("Visibility %1: %2").arg(node.path, verb);
  }
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
    command = cmd("rename_feature", {{QStringLiteral("uid"), node.uid}, {QStringLiteral("name"), name}});
    break;
  case BrowserNode::Type::NamedView:
    command = cmd("rename_named_view",
                  {{QStringLiteral("name"), node.uid.mid(6)}, {QStringLiteral("new_name"), name}});
    break;
  default:
    return;
  }
  if (m_host.runModelCommand(command)) {
    qDebug().noquote() << QStringLiteral("Renamed %1 to %2").arg(node.name, name);
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
    m_host.editFeature(node.uid);
    break;
  case BrowserNode::Type::Body:
  case BrowserNode::Type::Component:
    if (canChange()) {
      m_browser->startRename(node);
    }
    break;
  case BrowserNode::Type::Units:
    changeUnits();
    break;
  case BrowserNode::Type::NamedView:
    m_host.showNamedView(node.uid);
    break;
  default:
    break;
  }
}

void BrowserController::deleteNode(const BrowserNode& node) {
  switch (node.type) {
  case BrowserNode::Type::Body:
    // The feature that made it goes, as nothing removes a body by itself.
    deleteFeature(node.uid.section(QLatin1Char('.'), 0, 0));
    break;
  case BrowserNode::Type::Sketch:
  case BrowserNode::Type::Datum:
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
  default:
    break;
  }
}

void BrowserController::newComponent(const BrowserNode& node) {
  if (!canChange()) {
    return;
  }
  QJsonArray commands;
  if (node.uid != m_snapshot.activeComponent) {
    // It is made in the component the menu was opened on.
    commands.append(cmd("activate_component", {{QStringLiteral("component"), node.uid}}));
  }
  commands.append(cmd("create_component"));
  if (m_host.runModelCommands(commands, tr("New Component"))) {
    qDebug().noquote() << QStringLiteral("New component in %1").arg(node.name);
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
  const QString unit = QInputDialog::getItem(m_window, tr("Units"), tr("Document length unit:"), kLengthUnits,
                                             std::max(0, static_cast<int>(kLengthUnits.indexOf(m_snapshot.lengthUnit))),
                                             false, &ok);
  if (ok && unit != m_snapshot.lengthUnit &&
      m_host.runModelCommand(cmd("set_units", {{QStringLiteral("length"), unit}}))) {
    qDebug().noquote() << QStringLiteral("Units: %1").arg(unit);
  }
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
  const auto findInTimeline = [&](const QString& feature) {
    menu.add(QStringLiteral("timeline"), tr("Find in Timeline"), [this, feature] {
      m_timeline->selectFeature(feature);
      qDebug().noquote() << QStringLiteral("Found %1 in the timeline").arg(feature);
    });
  };
  const bool plane = item.kind == SelectKind::Plane;

  switch (node.type) {
  case BrowserNode::Type::Component: {
    const bool root = node.occurrence.isEmpty();
    if (m_snapshot.activeComponent != node.uid) {
      menu.add(QStringLiteral("radio-on"), tr("Activate Component"), [this, node] { activate(node); });
    }
    menu.add(QStringLiteral("component"), tr("New Component"), [this, node] { newComponent(node); });
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
    menu.separator();
    if (!root) {
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
    }
    isolate();
    menu.separator();
    renameEntry();
    if (!root) {
      menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    }
    break;
  }
  case BrowserNode::Type::Body: {
    const QString feature = node.uid.section(QLatin1Char('.'), 0, 0);
    const DocumentSnapshot::Feature* made = m_snapshot.feature(feature);
    menu.add(QStringLiteral("edit"), tr("Edit %1").arg(made != nullptr ? made->name : feature),
             [this, feature] { m_host.editFeature(feature); });
    menu.separator();
    visibility();
    isolate();
    menu.add(QStringLiteral("component"), tr("Create Components from Bodies"), [this, node] {
      QJsonObject result;
      if (canChange() &&
          m_host.runModelCommand(cmd("components_from_bodies", {{QStringLiteral("bodies"), QJsonArray{node.uid}}}),
                                 &result)) {
        qDebug().noquote() << QStringLiteral("Component from %1").arg(node.name);
      }
    });
    menu.separator();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    findInTimeline(feature);
    break;
  }
  case BrowserNode::Type::Sketch:
    menu.add(QStringLiteral("sketch"), tr("Edit Sketch"), [this, node] { m_host.editFeature(node.uid); });
    menu.add(QStringLiteral("look-at"), tr("Look At"), [this, item] { m_host.lookAt(item); });
    menu.add(QStringLiteral("export"), tr("Save As DXF"), [this, node] { m_host.exportSketch(node.uid); });
    menu.separator();
    visibility();
    renameEntry();
    menu.add(QStringLiteral("delete"), tr("Delete"), [this, node] { deleteNode(node); });
    findInTimeline(node.uid);
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
    findInTimeline(node.uid);
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
  case BrowserNode::Type::Construction:
    menu.add(node.visible ? QStringLiteral("eye-off") : QStringLiteral("eye"),
             node.visible ? tr("Hide All") : tr("Show All"), [this, node] { toggleVisibility(node); });
    break;
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
  default:
    break;
  }
  if (!menu.isEmpty()) {
    menu.exec(at);
  }
}

// ---------------------------------------------------------------------------
// The timeline

void BrowserController::featuresClicked(const QStringList& uids) {
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
    m_host.pickItems(items);
    return;
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
  m_host.pickItems(items);
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
    box.exec();
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

void BrowserController::pickItems(const Selection& items) { m_host.pickItems(items); }

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

void BrowserController::timelineMenu(const QString& uid, const QPoint& at) {
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
  menu.add(QStringLiteral("edit"), tr("Rename"), [this, uid] {
    if (canChange()) {
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
  menu.add(QStringLiteral("delete"), tr("Delete"), [this, uid] { deleteFeature(uid); });
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
  menu.add(QStringLiteral("browser"), tr("Find in Browser"), [this, uid] {
    const DocumentSnapshot::Feature* f = m_snapshot.feature(uid);
    if (m_browser->reveal(uid)) {
      qDebug().noquote() << QStringLiteral("Found %1 in the browser").arg(f != nullptr ? f->name : uid);
    } else {
      m_host.showStatus(tr("%1 has nothing in the browser.").arg(f != nullptr ? f->name : uid), false);
    }
  });
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
    const QString renamed = QInputDialog::getText(m_window, tr("Rename Group"), tr("Group name:"),
                                                  QLineEdit::Normal, name, &ok)
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

void BrowserController::openParameters() {
  if (!m_parameters) {
    m_parameters = new ParametersDialog(m_host, m_window);
  }
  m_parameters->show();
  m_parameters->raise();
  m_parameters->activateWindow();
  qDebug().noquote() << "Parameters dialog opened";
  m_parameters->refresh();
}

} // namespace mitcad
