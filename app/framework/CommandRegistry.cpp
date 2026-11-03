// SPDX-License-Identifier: MIT
#include "CommandRegistry.hpp"

#include <algorithm>

#include <QAction>
#include <QJsonObject>
#include <QSettings>
#include <QWidget>
#include <QtLogging>

#include "Icons.hpp"

namespace mitcad {
namespace {

const QString kShortcutGroup = QStringLiteral("shortcuts");

QString settingsKey(const QString& id) { return kShortcutGroup + QLatin1Char('/') + id; }

// A command's key in one context where its keys differ by context
// (mitcad#720); "shortcuts/<id>" stays the key of every context.
QString contextSettingsKey(const QString& id, unsigned context) {
  const QString group = context == SketchShortcutContext    ? QStringLiteral("shortcuts.sketch")
                        : context == CommandShortcutContext ? QStringLiteral("shortcuts.commands")
                                                            : QStringLiteral("shortcuts.model");
  return group + QLatin1Char('/') + id;
}

QKeySequence storedKey(const QSettings& settings, const QString& key) {
  return QKeySequence(settings.value(key).toString(), QKeySequence::PortableText);
}

} // namespace

CommandRegistry::CommandRegistry(QObject* parent) : QObject(parent) {}

CommandRegistry::~CommandRegistry() = default;

void CommandRegistry::add(const CommandDef& def) {
  m_commands.push_back(std::make_unique<CommandDef>(def));
}

const CommandDef* CommandRegistry::find(const QString& id) const {
  const QString current = m_aliases.value(id, id);
  for (const auto& def : m_commands) {
    if (def->id == current) {
      return def.get();
    }
  }
  return nullptr;
}

void CommandRegistry::alias(const QString& oldId, const QString& id) {
  if (oldId == id) {
    return;
  }
  m_aliases.insert(oldId, id);
  // A shortcut saved under the former id (a custom one only: the default
  // is the command's own) is the new command's, unless that has one; so
  // are its keys per context.
  QSettings settings;
  const auto move = [&settings](const QString& from, const QString& to) {
    if (settings.contains(from)) {
      if (!settings.contains(to)) {
        settings.setValue(to, settings.value(from));
      }
      settings.remove(from);
    }
  };
  move(settingsKey(oldId), settingsKey(id));
  for (const unsigned context : shortcutContextList(AllShortcutContexts)) {
    move(contextSettingsKey(oldId, context), contextSettingsKey(id, context));
  }
}

const CommandDef* CommandRegistry::editorOf(const QJsonObject& feature) const {
  const QString type = feature.value(QStringLiteral("type")).toString();
  for (const auto& def : m_commands) {
    if (def->kind == CommandDef::Kind::Feature && !type.isEmpty() &&
        (def->featureType == type || def->editsAlso.contains(type)) && def->load &&
        (!def->canEdit || def->canEdit(feature))) {
      return def.get();
    }
  }
  return nullptr;
}

void CommandRegistry::createActions(QWidget* owner,
                                    const std::function<void(const QString&)>& trigger) {
  for (const auto& def : m_commands) {
    const QIcon icon = themeIcon(def->icon);
    if (icon.isNull()) {
      qWarning().noquote() << QStringLiteral("Command %1 has no icon '%2'").arg(def->id, def->icon);
    }
    auto* action = new QAction(icon, def->name, owner);
    action->setObjectName(QStringLiteral("command_") + def->id);
    const QString id = def->id;
    connect(action, &QAction::triggered, owner, [trigger, id] { trigger(id); });
    // Shortcuts work wherever the keyboard is in the window.
    action->setShortcutContext(Qt::WindowShortcut);
    owner->addAction(action);
    m_actions.insert(id, action);
  }
  refreshShortcuts();
}

QKeySequence CommandRegistry::defaultShortcut(const QString& id) const {
  const CommandDef* def = find(id);
  return def != nullptr ? def->shortcut : QKeySequence();
}

unsigned CommandRegistry::contextsOf(const CommandDef& def) const {
  unsigned contexts = def.mode == CommandDef::Mode::Sketch ? SketchShortcutContext : ModelShortcutContext;
  if (def.mode == CommandDef::Mode::Any) contexts = ModelShortcutContext | SketchShortcutContext;
  if (def.mode == CommandDef::Mode::Model && def.kind == CommandDef::Kind::Feature) {
    // Keep this in step with MainWindow::isAvailable(): Extrude/Revolve can
    // finish a sketch and start a feature, unlike model Move/Copy.
    const InputDef* input = def.primarySelection();
    if (input != nullptr && input->filter.testFlag(SelectKind::Profile)) contexts |= SketchShortcutContext;
  }
  if (def.duringCommands) contexts |= CommandShortcutContext;
  return contexts;
}

unsigned CommandRegistry::shortcutContexts(const QString& id) const {
  const CommandDef* def = find(id);
  return def != nullptr ? contextsOf(*def) : 0u;
}

unsigned CommandRegistry::activeContext(unsigned contexts) const {
  if ((contexts & m_context) != 0) return m_context;
  const QList<unsigned> list = shortcutContextList(contexts);
  return list.isEmpty() ? unsigned(ModelShortcutContext) : list.front();
}

void CommandRegistry::setShortcutContext(unsigned context) {
  if (context == m_context) return;
  m_context = context;
  const QSettings settings;
  for (const auto& def : m_commands) {
    // Only a command of several contexts (more than one bit) changes keys.
    const unsigned contexts = contextsOf(*def);
    if ((contexts & (contexts - 1)) != 0) applyShortcut(*def, settings);
  }
}

QKeySequence CommandRegistry::savedShortcut(const QSettings& settings, const CommandDef& def,
                                            unsigned context) const {
  const QString own = contextSettingsKey(def.id, context);
  if ((contextsOf(def) & context) != 0 && settings.contains(own)) return storedKey(settings, own);
  if (settings.contains(settingsKey(def.id))) return storedKey(settings, settingsKey(def.id));
  return def.shortcut;
}

QKeySequence CommandRegistry::shortcut(const QString& id, unsigned context) const {
  const CommandDef* def = find(id);
  if (def == nullptr) return QKeySequence();
  const QSettings settings;
  return savedShortcut(settings, *def, context);
}

QKeySequence CommandRegistry::shortcut(const QString& id) const {
  const CommandDef* def = find(id);
  return def != nullptr ? shortcut(id, activeContext(contextsOf(*def))) : QKeySequence();
}

ContextShortcuts CommandRegistry::shortcuts(const QString& id) const {
  ContextShortcuts keys;
  const CommandDef* def = find(id);
  if (def == nullptr) return keys;
  const QSettings settings;
  for (const unsigned context : shortcutContextList(contextsOf(*def))) {
    keys.insert(context, savedShortcut(settings, *def, context));
  }
  return keys;
}

ShortcutBinding CommandRegistry::binding(const CommandDef& def, unsigned contexts,
                                         const QKeySequence& primary) const {
  return {def.id, def.name, contexts, primary, def.shortcut, def.alternates, false};
}

QList<ShortcutBinding> CommandRegistry::bindings(const CommandDef& def, const ContextShortcuts& keys) const {
  QList<ShortcutBinding> list;
  for (auto it = keys.cbegin(); it != keys.cend(); ++it) {
    const auto same = std::find_if(list.begin(), list.end(),
                                   [&it](const ShortcutBinding& entry) { return entry.primary == it.value(); });
    if (same != list.end()) {
      same->contexts |= it.key();
    } else {
      list.append(binding(def, it.key(), it.value()));
    }
  }
  return list;
}

QList<QKeySequence> CommandRegistry::effectiveShortcuts(const QString& id,
                                                        const QKeySequence& primary) const {
  const CommandDef* def = find(id);
  return def != nullptr ? binding(*def, contextsOf(*def), primary).effectiveKeys() : QList<QKeySequence>();
}

QList<QKeySequence> CommandRegistry::effectiveKeys(const CommandDef& def, const QSettings& settings) const {
  if (m_disabledShortcuts.contains(def.id)) return {};
  const unsigned context = activeContext(contextsOf(def));
  return binding(def, context, savedShortcut(settings, def, context)).effectiveKeys();
}

QList<QKeySequence> CommandRegistry::effectiveShortcuts(const QString& id) const {
  const CommandDef* def = find(id);
  if (def == nullptr) return {};
  const QSettings settings;
  return effectiveKeys(*def, settings);
}

QString CommandRegistry::shortcutHint(const QString& id) const {
  const CommandDef* def = find(id);
  if (def == nullptr) return QString();
  const QList<QKeySequence> keys = effectiveShortcuts(id);
  return keys.isEmpty() ? def->name
                        : tr("%1 (%2)").arg(def->name, keys.front().toString(QKeySequence::NativeText));
}

QList<ShortcutConflict> CommandRegistry::shortcutConflicts(const QHash<QString, ContextShortcuts>& proposed) const {
  const QSettings settings;
  QList<ShortcutBinding> all = fixedShortcutBindings();
  for (const auto& def : m_commands) {
    const ContextShortcuts wanted = proposed.value(def->id);
    ContextShortcuts keys;
    for (const unsigned context : shortcutContextList(contextsOf(*def))) {
      keys.insert(context, wanted.contains(context) ? wanted.value(context) : savedShortcut(settings, *def, context));
    }
    // A command's keys meet another's only in the contexts that have them.
    all.append(bindings(*def, keys));
  }
  return mitcad::shortcutConflicts(all);
}

QList<ShortcutConflict> CommandRegistry::defaultShortcutConflicts() const {
  QHash<QString, ContextShortcuts> defaults;
  for (const auto& def : m_commands) {
    ContextShortcuts& keys = defaults[def->id];
    for (const unsigned context : shortcutContextList(contextsOf(*def))) keys.insert(context, def->shortcut);
  }
  return shortcutConflicts(defaults);
}

void CommandRegistry::setShortcut(const QString& id, const QKeySequence& sequence) {
  const CommandDef* def = find(id);
  if (def == nullptr) return;
  ContextShortcuts keys;
  for (const unsigned context : shortcutContextList(contextsOf(*def))) keys.insert(context, sequence);
  QHash<QString, ContextShortcuts> proposal;
  proposal.insert(def->id, keys);
  setShortcuts(proposal);
}

void CommandRegistry::setShortcut(const QString& id, unsigned context, const QKeySequence& sequence) {
  ContextShortcuts keys;
  keys.insert(context, sequence);
  QHash<QString, ContextShortcuts> proposal;
  proposal.insert(id, keys);
  setShortcuts(proposal);
}

void CommandRegistry::setShortcuts(const QHash<QString, ContextShortcuts>& proposal) {
  QSettings settings;
  for (auto it = proposal.cbegin(); it != proposal.cend(); ++it) {
    const CommandDef* def = find(it.key());
    if (def == nullptr) continue;
    ContextShortcuts keys;
    for (const unsigned context : shortcutContextList(contextsOf(*def))) {
      keys.insert(context, it.value().contains(context) ? it.value().value(context)
                                                        : savedShortcut(settings, *def, context));
    }
    for (const unsigned context : shortcutContextList(AllShortcutContexts)) {
      settings.remove(contextSettingsKey(def->id, context));
    }
    const QList<QKeySequence> values = keys.values();
    const bool same = std::all_of(values.cbegin(), values.cend(),
                                  [&values](const QKeySequence& key) { return key == values.front(); });
    if (same) {
      // One key in every context: saved as before mitcad#720.
      const QKeySequence key = values.value(0);
      if (key == def->shortcut) settings.remove(settingsKey(def->id));
      else settings.setValue(settingsKey(def->id), key.toString(QKeySequence::PortableText));
    } else {
      settings.remove(settingsKey(def->id));
      for (auto key = keys.cbegin(); key != keys.cend(); ++key) {
        if (key.value() != def->shortcut) {
          settings.setValue(contextSettingsKey(def->id, key.key()), key.value().toString(QKeySequence::PortableText));
        }
      }
    }
  }
  refreshShortcuts();
}

void CommandRegistry::refreshShortcuts() {
  const QList<ShortcutConflict> conflicts = shortcutConflicts();
  m_disabledShortcuts = conflictingShortcutIds(conflicts);
  m_shortcutWarnings.clear();
  for (const auto& conflict : conflicts) {
    m_shortcutWarnings.append(conflict.description());
  }
  const QSettings settings;
  for (const auto& def : m_commands) {
    // A command's keys that differ by context, in the log (the UI tests).
    const QList<unsigned> contexts = shortcutContextList(contextsOf(*def));
    if (contexts.size() > 1) {
      const QKeySequence first = savedShortcut(settings, *def, contexts.front());
      bool differ = false;
      QStringList parts;
      for (const unsigned context : contexts) {
        const QKeySequence key = savedShortcut(settings, *def, context);
        differ = differ || key != first;
        parts.append(QStringLiteral("%1 %2").arg(shortcutContextName(context),
                                                 key.isEmpty() ? QStringLiteral("(none)")
                                                               : key.toString(QKeySequence::PortableText)));
      }
      if (differ) {
        qInfo().noquote() << QStringLiteral("Shortcut keys of %1 by context: %2")
                                 .arg(def->id, parts.join(QStringLiteral(", ")));
      }
    }
    applyShortcut(*def, settings);
  }
  if (!m_shortcutWarnings.isEmpty()) {
    qWarning().noquote() << tr("Conflicting saved shortcuts are disabled; commands remain in menus. "
                               "Open Keyboard Shortcuts to repair them.")
                        << m_shortcutWarnings.join(QLatin1Char('\n'));
  }
}

void CommandRegistry::applyShortcut(const CommandDef& def, const QSettings& settings) {
  QAction* action = m_actions.value(def.id);
  if (action == nullptr) return;
  const QList<QKeySequence> keys = effectiveKeys(def, settings);
  action->setShortcuts(keys);
  // The name before the description without its ellipsis ("Processes: …",
  // not "Processes…: …"), as the quick-access tooltips write it.
  QString name = def.name;
  while (!def.tooltip.isEmpty() && (name.endsWith(QChar(0x2026)) || name.endsWith(QLatin1Char('.')))) {
    name.chop(1);
  }
  QString tip = def.tooltip.isEmpty() ? def.name : QStringLiteral("%1: %2").arg(name, def.tooltip);
  if (!keys.isEmpty()) {
    tip += QStringLiteral(" (%1)").arg(keys.front().toString(QKeySequence::NativeText));
  } else if (m_disabledShortcuts.contains(def.id)) {
    tip += tr(" (conflicting shortcut disabled; change it in Keyboard Shortcuts)");
  }
  action->setToolTip(tip);
}

void CommandRegistry::updateEnabled(const std::function<bool(const CommandDef&)>& available) {
  for (const auto& def : m_commands) {
    if (QAction* action = m_actions.value(def->id)) {
      action->setEnabled(available(*def));
    }
  }
}

} // namespace mitcad
