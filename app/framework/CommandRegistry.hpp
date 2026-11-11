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
#include "ShortcutBindings.hpp"

class QAction;
class QSettings;
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
  // A command by its id, or by an id it replaced (alias()).
  const CommandDef* find(const QString& id) const;
  // `oldId` is a former id of the command `id` (mitcad#503: Move to a
  // Project became Save to Project): find() and action() accept it, and
  // a shortcut saved under it moves to `id` (kept when `id` has one of its
  // own).
  void alias(const QString& oldId, const QString& id);
  // The feature command that can edit a definition (the `feature` query's
  // def), or null.
  const CommandDef* editorOf(const QJsonObject& def) const;

  // Makes an action per command, with window-wide shortcuts on `owner`;
  // triggering one calls `trigger` with the command's id.
  void createActions(QWidget* owner, const std::function<void(const QString&)>& trigger);
  QAction* action(const QString& id) const { return m_actions.value(m_aliases.value(id, id)); }

  // The contexts a command's keys live in (ShortcutContext bits): Model,
  // Sketch or both by its mode, a profile feature's Sketch too, and During
  // commands for one that runs beside a panel. A command of several
  // contexts has a key in each (mitcad#720), the same one unless changed.
  unsigned shortcutContexts(const QString& id) const;
  // The context whose keys the actions carry (MainWindow: by its mode).
  // A command of several contexts that is not in it carries its first
  // context's key (its action is disabled there).
  unsigned shortcutContext() const { return m_context; }
  void setShortcutContext(unsigned context);
  // An overlay context active beside it (mitcad#1661:
  // Sketch3dToolShortcutContext while a 3D sketch's drawing tool is
  // active), 0 for none: the keys of its commands are theirs alone, the
  // other commands' actions drop the same keys meanwhile (Turn View Up's
  // Up). Logged "Shortcut overlay: 3D Sketch Tool" ("none") when it changes.
  void setShortcutOverlay(unsigned contexts);

  // A command's key in one context: saved for that context, else saved for
  // every context, else its default.
  QKeySequence shortcut(const QString& id, unsigned context) const;
  // Its key in the current context (or its first).
  QKeySequence shortcut(const QString& id) const;
  // Its key in each of its contexts.
  ContextShortcuts shortcuts(const QString& id) const;
  QKeySequence defaultShortcut(const QString& id) const;
  // The runtime bindings in the current context, including default-only
  // alternates. Conflicting legacy settings remain editable but are not
  // installed on actions.
  QList<QKeySequence> effectiveShortcuts(const QString& id) const;
  QList<QKeySequence> effectiveShortcuts(const QString& id, const QKeySequence& primary) const;
  // `proposed`: keys by command and context; the others as saved.
  QList<ShortcutConflict> shortcutConflicts(const QHash<QString, ContextShortcuts>& proposed = {}) const;
  // Registered defaults, independent of any saved user overrides.
  QList<ShortcutConflict> defaultShortcutConflicts() const;
  const QStringList& shortcutWarnings() const { return m_shortcutWarnings; }
  QString shortcutHint(const QString& id) const;
  // Applies one complete proposal so swaps do not create intermediate
  // conflicts. A command's contexts left out keep their keys; one key in
  // all its contexts is saved as before ("shortcuts/<id>"), different keys
  // per context ("shortcuts.model/<id>", "shortcuts.sketch/<id>",
  // "shortcuts.commands/<id>").
  void setShortcuts(const QHash<QString, ContextShortcuts>& proposal);
  // Changes a shortcut in every context of the command and saves it; an
  // empty sequence removes it.
  void setShortcut(const QString& id, const QKeySequence& shortcut);
  // Changes a shortcut in one context.
  void setShortcut(const QString& id, unsigned context, const QKeySequence& shortcut);

  // Enables the actions of the commands `available` accepts.
  void updateEnabled(const std::function<bool(const CommandDef&)>& available);

private:
  unsigned contextsOf(const CommandDef& def) const;
  // The context whose key a command of `contexts` carries now.
  unsigned activeContext(unsigned contexts) const;
  QKeySequence savedShortcut(const QSettings& settings, const CommandDef& def, unsigned context) const;
  ShortcutBinding binding(const CommandDef& def, unsigned contexts, const QKeySequence& primary) const;
  // One binding per distinct key, with the contexts that have it.
  QList<ShortcutBinding> bindings(const CommandDef& def, const ContextShortcuts& keys) const;
  QList<QKeySequence> effectiveKeys(const CommandDef& def, const QSettings& settings) const;
  void refreshShortcuts();
  void applyShortcut(const CommandDef& def, const QSettings& settings);
  // The keys of the active overlay context's commands (m_overlayKeys).
  void updateOverlayKeys(const QSettings& settings);

  std::vector<std::unique_ptr<CommandDef>> m_commands;
  QHash<QString, QAction*> m_actions;
  QHash<QString, QString> m_aliases; // former id -> id
  QSet<QString> m_disabledShortcuts;
  QStringList m_shortcutWarnings;
  unsigned m_context = ModelShortcutContext;
  unsigned m_overlay = 0;              // mitcad#1661
  QList<QKeySequence> m_overlayKeys; // its commands' keys
};

} // namespace mitcad
