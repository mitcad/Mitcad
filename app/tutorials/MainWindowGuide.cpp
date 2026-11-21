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
  tutorials.tooltip = tr("Lists the tutorials of the tutorial sources and opens one beside the design");
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
  leave.enabled = [this] { return (m_guide != nullptr && m_guide->active()) || tutorialOpen(); };
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
  m_guide = new TutorialGuide(std::move(host), this);
  // Leave Tutorial's availability follows the step.
  connect(m_guide, &TutorialGuide::changed, this, [this] { updateActions(); });
  return m_guide;
}

bool MainWindow::startGuideStep(const QJsonObject& json, QString* error) {
  const GuideStep step = GuideStep::parse(json, error);
  if (step.id.isEmpty()) {
    return false;
  }
  for (const QString& id : step.commands() + step.allowed) {
    if (m_registry->find(id) == nullptr) {
      *error = QStringLiteral("MitCad has no command %1").arg(id);
      return false;
    }
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
