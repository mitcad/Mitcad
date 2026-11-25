// SPDX-License-Identifier: MIT
// In-app tutorials (mitcad#2012, docs/tutorials.md): the guide step shown
// in a design window (TutorialGuide), its target found as the UI names it
// (a command's visible button, an input of the open panel, a Browser row,
// the view), what it asks of the window (the open command and its inputs,
// the picks, whether a command is open), and Help › Leave Tutorial.
#include "../MainWindow.hpp"

#include <optional>
#include <stdexcept>
#include <utility>

#include <QAbstractButton>
#include <QAction>
#include <QMenu>
#include <QTreeWidget>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Ribbon.hpp"
#include "GuideStep.hpp"
#include "TutorialGuide.hpp"

namespace mitcad {
namespace {

// `widget`'s rectangle in `window`'s coordinates, across windows (a card of
// native glass is a window of its own).
QRect rectIn(const QWidget* widget, const QWidget* window, const QRect& local) {
  return QRect(window->mapFromGlobal(widget->mapToGlobal(local.topLeft())), local.size());
}

} // namespace

void MainWindow::registerTutorialCommands() {
  CommandDef tutorials;
  tutorials.id = QStringLiteral("help.tutorials");
  tutorials.name = tr("Tutorials…");
  tutorials.icon = QStringLiteral("web");
  tutorials.tooltip = tr("Shows the tutorials on the Start page, where one starts in a design of its own");
  tutorials.kind = CommandDef::Kind::Action;
  tutorials.mode = CommandDef::Mode::Any;
  tutorials.keywords = {QStringLiteral("tutorial"), QStringLiteral("lesson"), QStringLiteral("learn"),
                        QStringLiteral("course"), QStringLiteral("guide"), QStringLiteral("help")};
  tutorials.run = [this] { showTutorials(); };
  m_registry->add(tutorials);

  CommandDef leave;
  leave.id = QString::fromLatin1(kLeaveTutorialCommand);
  leave.name = tr("Leave Tutorial");
  leave.icon = QStringLiteral("close");
  leave.tooltip = tr("Ends the tutorial and its step; all of MitCad's input is free again");
  leave.kind = CommandDef::Kind::Action;
  leave.mode = CommandDef::Mode::Any;
  leave.keywords = {QStringLiteral("tutorial"), QStringLiteral("guide"), QStringLiteral("lesson")};
  leave.duringCommands = true;
  leave.enabled = [this] {
    return (m_guide != nullptr && (m_guide->active() || m_guide->holding())) || tutorialOpen();
  };
  leave.run = [this] { leaveTutorial(); };
  m_registry->add(leave);
}

TutorialGuide* MainWindow::guide() {
  if (m_guide != nullptr) {
    return m_guide;
  }
  GuideHost host;
  host.window = this;
  host.viewer = m_viewer;
  host.targetRect = [this](const GuideStep& step) { return guideTargetRect(step); };
  host.openCommand = [this] { return m_session ? m_session->def().id : QString(); };
  host.inputText = [this](const QString& key) {
    if (!m_session) {
      return QString();
    }
    const QString text = m_session->state().text(key);
    return text.isEmpty() ? m_session->state().expression(key) : text;
  };
  host.pickedKinds = [this] {
    QStringList kinds;
    for (const SelectionItem& item : m_selection) {
      QString kind = SelectionItem::kindName(item.kind);
      kind.replace(QLatin1Char(' '), QLatin1Char('_'));
      if (!kinds.contains(kind)) {
        kinds << kind;
      }
    }
    return kinds;
  };
  host.idle = [this] { return m_mode == Mode::Idle && !m_session; };
  host.mode = [this] {
    // DocumentSession::Mode's order.
    static const char* const modes[] = {"idle", "pick_plane", "sketch", "command"};
    return QString::fromLatin1(modes[static_cast<int>(m_mode)]);
  };
  host.leave = [this] { trigger(QString::fromLatin1(kLeaveTutorialCommand)); };
  // The toolbar's workspace and tab (mitcad#2370).
  host.workspaceListOpen = [this] { return m_ribbon != nullptr && m_ribbon->workspaceListOpen(); };
  host.workspace = [this] { return m_ribbon != nullptr ? m_ribbon->workspace() : QString(); };
  host.tab = [this] { return m_ribbon != nullptr ? m_ribbon->currentTab() : QString(); };
  host.navigationPress = [this](const QPoint& position, Qt::MouseButton button, Qt::KeyboardModifiers keys) {
    return m_viewer != nullptr && m_viewer->navigationPress(position, button, keys);
  };
  m_guide = new TutorialGuide(std::move(host), this);
  // The user's orbit, pan or zoom of the view (mitcad#2369): emitted while
  // the view paints, so queued.
  if (m_viewer != nullptr) {
    connect(m_viewer, &OcctViewer::viewNavigated, m_guide, &TutorialGuide::viewNavigated, Qt::QueuedConnection);
  }
  // Leave Tutorial's availability follows the step; a workspace step's
  // entry is outlined in the selector's list (mitcad#2370), which covers
  // the window's outline.
  connect(m_guide, &TutorialGuide::changed, this, [this] {
    updateActions();
    if (m_ribbon != nullptr) {
      const bool workspaceStep = m_guide->active() && m_guide->step().target == GuideStep::Target::Workspace;
      m_ribbon->setGuideWorkspace(workspaceStep ? m_guide->step().targetName : QString());
    }
  });
  // The datums' places for UI tests keep clear of the callout (mitcad#2311),
  // as of the cards over the view (OcctViewer::coveredChanged).
  connect(m_guide, &TutorialGuide::calloutMoved, this, [this] { logDatumPlaces(); }, Qt::QueuedConnection);
  return m_guide;
}

bool MainWindow::startGuideStep(const QJsonObject& json, QString* error) {
  GuideStep step = GuideStep::parse(json, error);
  if (step.id.isEmpty()) {
    return false;
  }
  for (const QString& id : step.commands() + step.allowed) {
    if (m_registry->find(id) == nullptr) {
      *error = QStringLiteral("MitCad has no command %1").arg(id);
      return false;
    }
  }
  // The toolbar's places (mitcad#2370): the selector and the workspaces
  // where the layout has a selector; workspaces and tabs by their names,
  // kept as the toolbar's keys ("DESIGN", "SOLID").
  const bool selectorStep = step.target == GuideStep::Target::WorkspaceSelector ||
                            step.target == GuideStep::Target::Workspace || step.done == GuideStep::Done::Opened ||
                            step.done == GuideStep::Done::Chosen;
  if (selectorStep && (m_ribbon == nullptr || m_ribbon->workspaceSelector() == nullptr)) {
    *error = QStringLiteral("The toolbar of this layout has no workspace selector");
    return false;
  }
  const auto workspaceKey = [error](QString* name) {
    const QString key = Ribbon::workspaceOfName(*name);
    if (key.isEmpty()) {
      QStringList names;
      for (const QString& workspace : Ribbon::workspaces()) {
        names << Ribbon::workspaceTitle(workspace);
      }
      *error = QStringLiteral("MitCad has no workspace %1; the workspaces are %2")
                   .arg(*name, names.join(QStringLiteral(", ")));
      return false;
    }
    *name = key;
    return true;
  };
  const auto tabKey = [error](QString* name) {
    const QString key = Ribbon::tabOfName(*name);
    if (key.isEmpty()) {
      QStringList names;
      for (const QString& tab : Ribbon::guideTabs()) {
        names << displayName(tab);
      }
      *error = QStringLiteral("MitCad has no tab %1; the tabs are %2").arg(*name, names.join(QStringLiteral(", ")));
      return false;
    }
    *name = key;
    return true;
  };
  if ((step.target == GuideStep::Target::Workspace && !workspaceKey(&step.targetName)) ||
      (step.done == GuideStep::Done::Chosen && !workspaceKey(&step.doneName)) ||
      (step.target == GuideStep::Target::Tab && !tabKey(&step.targetName)) ||
      (step.done == GuideStep::Done::Activated && !tabKey(&step.doneName))) {
    return false;
  }
  guide()->start(step);
  return true;
}

void MainWindow::leaveTutorial() {
  if (m_guide != nullptr) {
    m_guide->end(QStringLiteral("leave"));
  }
  closeTutorialPane();
  qInfo().noquote() << QStringLiteral("Tutorial: left");
}

QRect MainWindow::guideTargetRect(const GuideStep& step) const {
  QWidget* host = const_cast<MainWindow*>(this);
  switch (step.target) {
  case GuideStep::Target::Command: {
    // Its button where one is shown: on the ribbon, a toolbar, a palette;
    // not a menu's item (a menu is a window of its own, over the outline).
    QAction* action = m_registry->action(step.targetName);
    if (action == nullptr) {
      return QRect();
    }
    for (QObject* object : action->associatedObjects()) {
      // A button that shows it (the ribbon's, a toolbar's, a palette's):
      // not a menu, nor a window that carries the action for its shortcut.
      auto* widget = qobject_cast<QAbstractButton*>(object);
      if (widget == nullptr || !widget->isVisible() || widget->width() <= 0) {
        continue;
      }
      return rectIn(widget, host, widget->rect());
    }
    return QRect();
  }
  case GuideStep::Target::Input: {
    if (!m_session) {
      return QRect();
    }
    const std::optional<QPoint> place = m_session->inputPlace(step.targetName);
    if (!place) {
      return QRect();
    }
    if (QWidget* field = host->childAt(*place); field != nullptr) {
      return rectIn(field, host, field->rect());
    }
    return QRect(*place - QPoint(12, 12), QSize(24, 24));
  }
  case GuideStep::Target::Browser: {
    auto* tree = host->findChild<QTreeWidget*>(QStringLiteral("modelBrowserTree"));
    if (tree == nullptr || !tree->isVisible()) {
      return QRect();
    }
    QList<QTreeWidgetItem*> rows = tree->findItems(step.targetName, Qt::MatchExactly | Qt::MatchRecursive);
    if (rows.isEmpty()) {
      rows = tree->findItems(step.targetName, Qt::MatchStartsWith | Qt::MatchRecursive);
    }
    for (QTreeWidgetItem* row : rows) {
      const QRect rect = tree->visualItemRect(row);
      if (!rect.isEmpty() && tree->viewport()->rect().intersects(rect)) {
        return rectIn(tree->viewport(), host, rect);
      }
    }
    return QRect();
  }
  case GuideStep::Target::View:
    return m_viewer != nullptr && m_viewer->isVisible() ? rectIn(m_viewer, host, m_viewer->rect()) : QRect();
  case GuideStep::Target::WorkspaceSelector:
  case GuideStep::Target::Workspace:
  case GuideStep::Target::Tab: {
    // The toolbar's places (mitcad#2370), in the layout shown: the toolbar
    // panel's (Floating and Docked on Windows and Linux) or the macOS
    // Docked toolbar's.
    if (m_ribbon == nullptr) {
      return QRect();
    }
    QWidget* selector = m_ribbon->workspaceSelector();
    const auto selectorRect = [host, selector] {
      return selector != nullptr && selector->isVisible() ? rectIn(selector, host, selector->rect()) : QRect();
    };
    if (step.target == GuideStep::Target::WorkspaceSelector) {
      return selectorRect();
    }
    if (step.target == GuideStep::Target::Workspace) {
      // Its entry while the selector's list is open (the list outlines it
      // too: it covers the window's outline), else the selector that opens
      // the list.
      if (QWidget* entry = m_ribbon->workspaceEntry(step.targetName); entry != nullptr && entry->isVisible()) {
        return rectIn(entry, host, entry->rect());
      }
      return selectorRect();
    }
    const QRect tab = m_ribbon->tabGlobalRect(step.targetName);
    if (!tab.isNull()) {
      return QRect(host->mapFromGlobal(tab.topLeft()), tab.size());
    }
    // A tab of another workspace: the selector that shows it.
    const QString of = Ribbon::workspaceOfTab(step.targetName);
    if (!of.isEmpty() && of != m_ribbon->workspace()) {
      return selectorRect();
    }
    return QRect();
  }
  case GuideStep::Target::Timeline: {
    // The timeline's dock (Docked) or card (Floating) while it is shown.
    for (QWidget* timeline : host->findChildren<QWidget*>(QStringLiteral("timelineDock"))) {
      if (timeline->isVisible() && !timeline->rect().isEmpty()) {
        return rectIn(timeline, host, timeline->rect());
      }
    }
    return QRect();
  }
  }
  return QRect();
}

QJsonObject MainWindow::mcpGuide(const QJsonObject& arguments) {
  const QString action = arguments.value(QStringLiteral("action")).toString(QStringLiteral("state"));
  if (action == QLatin1String("start")) {
    QString error;
    if (!startGuideStep(arguments.value(QStringLiteral("step")).toObject(), &error)) {
      throw std::runtime_error(error.toStdString());
    }
    qInfo().noquote() << QStringLiteral("MCP: guide step %1 started").arg(guide()->step().id);
  } else if (action == QLatin1String("end")) {
    guide()->end(QStringLiteral("ended"));
  }
  QJsonObject state = guide()->state();
  state.insert(QStringLiteral("document"), m_mcpDocumentId);
  return state;
}

} // namespace mitcad
