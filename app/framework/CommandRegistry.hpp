// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <vector>

#include <QHash>
#include <QKeySequence>
#include <QObject>
#include <QString>

#include "Command.hpp"

class QAction;
class QWidget;

namespace mitcad {

// Every command of the application with its action: the toolbar, the menus,
// the command search and the context menu use the same actions, and their
// shortcuts can be changed (saved in the settings, group "shortcuts").
class CommandRegistry : public QObject {
  Q_OBJECT

public:
  explicit CommandRegistry(QObject* parent = nullptr);
  ~CommandRegistry() override;

  // Adds a command; its action is made by createActions().
  void add(const CommandDef& def);
  const std::vector<std::unique_ptr<CommandDef>>& commands() const { return m_commands; }
  const CommandDef* find(const QString& id) const;
  // The feature command that can edit a definition (the `feature` query's
  // def), or null.
  const CommandDef* editorOf(const QJsonObject& def) const;

  // Makes an action per command, with window-wide shortcuts on `owner`;
  // triggering one calls `trigger` with the command's id.
  void createActions(QWidget* owner, const std::function<void(const QString&)>& trigger);
  QAction* action(const QString& id) const { return m_actions.value(id); }

  QKeySequence shortcut(const QString& id) const;
  QKeySequence defaultShortcut(const QString& id) const;
  // Changes a shortcut and saves it; an empty sequence removes it.
  void setShortcut(const QString& id, const QKeySequence& shortcut);

  // Enables the actions of the commands `available` accepts.
  void updateEnabled(const std::function<bool(const CommandDef&)>& available);

private:
  void applyShortcut(const CommandDef& def, const QKeySequence& shortcut);

  std::vector<std::unique_ptr<CommandDef>> m_commands;
  QHash<QString, QAction*> m_actions;
};

} // namespace mitcad
