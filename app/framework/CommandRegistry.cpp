// SPDX-License-Identifier: MIT
#include "CommandRegistry.hpp"

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

} // namespace

CommandRegistry::CommandRegistry(QObject* parent) : QObject(parent) {}

CommandRegistry::~CommandRegistry() = default;

void CommandRegistry::add(const CommandDef& def) {
  m_commands.push_back(std::make_unique<CommandDef>(def));
}

const CommandDef* CommandRegistry::find(const QString& id) const {
  for (const auto& def : m_commands) {
    if (def->id == id) {
      return def.get();
    }
  }
  return nullptr;
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
    applyShortcut(*def, shortcut(id));
  }
}

QKeySequence CommandRegistry::defaultShortcut(const QString& id) const {
  const CommandDef* def = find(id);
  return def != nullptr ? def->shortcut : QKeySequence();
}

QKeySequence CommandRegistry::shortcut(const QString& id) const {
  const QSettings settings;
  if (settings.contains(settingsKey(id))) {
    return QKeySequence(settings.value(settingsKey(id)).toString(), QKeySequence::PortableText);
  }
  return defaultShortcut(id);
}

void CommandRegistry::setShortcut(const QString& id, const QKeySequence& sequence) {
  const CommandDef* def = find(id);
  if (def == nullptr) {
    return;
  }
  QSettings settings;
  if (sequence == def->shortcut) {
    settings.remove(settingsKey(id));
  } else {
    settings.setValue(settingsKey(id), sequence.toString(QKeySequence::PortableText));
  }
  applyShortcut(*def, sequence);
}

void CommandRegistry::applyShortcut(const CommandDef& def, const QKeySequence& sequence) {
  QAction* action = m_actions.value(def.id);
  if (action == nullptr) {
    return;
  }
  QList<QKeySequence> keys;
  if (!sequence.isEmpty()) {
    keys.append(sequence);
    if (sequence == def.shortcut) {
      keys.append(def.alternates);
    }
  }
  action->setShortcuts(keys);
  QString tip = def.tooltip.isEmpty() ? def.name : QStringLiteral("%1: %2").arg(def.name, def.tooltip);
  if (!sequence.isEmpty()) {
    tip += QStringLiteral(" (%1)").arg(sequence.toString(QKeySequence::NativeText));
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
