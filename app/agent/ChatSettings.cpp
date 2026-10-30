// SPDX-License-Identifier: MIT
#include "ChatSettings.hpp"

#include <QDir>
#include <QSettings>
#include <QStandardPaths>

namespace mitcad::agent {
namespace {

QString alwaysKey(const QString& agent) { return QStringLiteral("chat/always/") + agent; }

QString folderFrom(const char* variable, QStandardPaths::StandardLocation location, const QString& name) {
  const QString given = qEnvironmentVariable(variable);
  if (!given.isEmpty()) {
    return QDir::cleanPath(QDir(given).absolutePath());
  }
  const QString data = QStandardPaths::writableLocation(location);
  // Never relative to the working directory, which may be a project's.
  return data.isEmpty() ? QString() : QDir(data).filePath(name);
}

} // namespace

QString approvalModeKey(ApprovalMode mode) {
  switch (mode) {
  case ApprovalMode::Ask: return QStringLiteral("ask");
  case ApprovalMode::ModelEdits: return QStringLiteral("model");
  case ApprovalMode::Everything: return QStringLiteral("all");
  }
  return QStringLiteral("model");
}

ApprovalMode approvalModeFromKey(const QString& key) {
  if (key == QStringLiteral("ask")) {
    return ApprovalMode::Ask;
  }
  if (key == QStringLiteral("all")) {
    return ApprovalMode::Everything;
  }
  return ApprovalMode::ModelEdits;
}

ChatSettings ChatSettings::load() {
  const QSettings settings;
  ChatSettings chosen;
  chosen.agent = settings.value(QStringLiteral("chat/agent"), chosen.agent).toString();
  chosen.approvals = approvalModeFromKey(settings.value(QStringLiteral("chat/approvals")).toString());
  chosen.history = settings.value(QStringLiteral("chat/history"), true).toBool();
  return chosen;
}

void ChatSettings::save() const {
  QSettings settings;
  settings.setValue(QStringLiteral("chat/agent"), agent);
  settings.setValue(QStringLiteral("chat/approvals"), approvalModeKey(approvals));
  settings.setValue(QStringLiteral("chat/history"), history);
}

QStringList alwaysAllowed(const QString& agent) {
  return QSettings().value(alwaysKey(agent)).toStringList();
}

void allowAlways(const QString& agent, const QString& kind) {
  QSettings settings;
  QStringList kinds = settings.value(alwaysKey(agent)).toStringList();
  if (!kinds.contains(kind)) {
    kinds << kind;
    settings.setValue(alwaysKey(agent), kinds);
  }
}

bool anyAlwaysAllowed() {
  QSettings settings;
  settings.beginGroup(QStringLiteral("chat/always"));
  const QStringList agents = settings.childKeys();
  for (const QString& agent : agents) {
    if (!settings.value(agent).toStringList().isEmpty()) {
      return true;
    }
  }
  return false;
}

void resetAlwaysAllowed() {
  QSettings settings;
  settings.remove(QStringLiteral("chat/always"));
}

QString chatModel(const QString& agent) {
  return QSettings().value(QStringLiteral("chat/model/") + agent).toString();
}

void setChatModel(const QString& agent, const QString& model) {
  QSettings settings;
  if (model.isEmpty()) {
    settings.remove(QStringLiteral("chat/model/") + agent);
  } else {
    settings.setValue(QStringLiteral("chat/model/") + agent, model);
  }
}

bool privacyNoticeShown(const QString& agent) {
  return QSettings().value(QStringLiteral("chat/notice/") + agent, false).toBool();
}

void setPrivacyNoticeShown(const QString& agent) {
  QSettings().setValue(QStringLiteral("chat/notice/") + agent, true);
}

QString chatDataDirectory() {
  return folderFrom("MITCAD_CHAT_DIR", QStandardPaths::AppDataLocation, QStringLiteral("chat"));
}

QString agentWorkDirectory() {
  return folderFrom("MITCAD_CHAT_AGENT_DIR", QStandardPaths::AppLocalDataLocation, QStringLiteral("chat-agent"));
}

ChatNotifier& ChatNotifier::instance() {
  static ChatNotifier notifier;
  return notifier;
}

} // namespace mitcad::agent
