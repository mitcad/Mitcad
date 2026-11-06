// SPDX-License-Identifier: MIT
#include "ChatPreferences.hpp"

#include <algorithm>

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QSpinBox>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "ApiAgent.hpp"
#include "ChatHistoryStore.hpp"

namespace mitcad::agent {

ChatPreferencesBox::ChatPreferencesBox(QWidget* parent)
#ifdef MITCAD_MAC_UI
    : QWidget(parent)
#else
    : QGroupBox(tr("Chat"), parent)
#endif
{
  setObjectName(QStringLiteral("chatPreferences"));
  const ChatSettings settings = ChatSettings::load();
  auto* form = new QFormLayout(this);
  m_agent = new QComboBox;
  m_agent->setObjectName(QStringLiteral("chatAgentSetting"));
  // The chat's agents (mitcad#947); the panel's switcher shows the ones
  // found on this computer.
  m_agent->addItem(tr("Claude Code"), QStringLiteral("claude"));
  m_agent->addItem(tr("Codex"), QStringLiteral("codex"));
  const QList<ApiAgent::Preset> presets = ApiAgent::presets();
  for (const ApiAgent::Preset& preset : presets) {
    m_agent->addItem(preset.name, preset.id);
  }
  m_agent->addItem(tr("Custom command"), QStringLiteral("acp"));
  // Another Claude Code found, chosen in the panel (mitcad#1289), stays
  // the choice.
  if (m_agent->findData(settings.agent) < 0 && settings.agent.startsWith(QStringLiteral("claude-"))) {
    m_agent->insertItem(1, tr("Claude Code (another one found)"), settings.agent);
  }
  m_agent->setCurrentIndex(std::max(0, m_agent->findData(settings.agent)));
  form->addRow(tr("Agent:"), m_agent);
  m_approvals = new QComboBox;
  m_approvals->setObjectName(QStringLiteral("chatApprovals"));
  m_approvals->addItem(tr("Ask every time"), approvalModeKey(ApprovalMode::Ask));
  m_approvals->addItem(tr("Model edits allowed"), approvalModeKey(ApprovalMode::ModelEdits));
  m_approvals->addItem(tr("Allow everything"), approvalModeKey(ApprovalMode::Everything));
  m_approvals->setCurrentIndex(std::max(0, m_approvals->findData(approvalModeKey(settings.approvals))));
  m_approvals->setToolTip(tr("Which of the agent's calls ask first; by default saving, opening and exporting"));
  form->addRow(tr("Approvals:"), m_approvals);
  m_history = new QCheckBox(tr("Keep the chat history of each design"));
  m_history->setObjectName(QStringLiteral("chatHistorySetting"));
  m_history->setChecked(settings.history);
  form->addRow(QString(), m_history);
  m_clear = new QPushButton(tr("Clear all history…"));
  m_clear->setObjectName(QStringLiteral("chatClearAll"));
  m_clear->setAutoDefault(false);
  m_reset = new QPushButton(tr("Reset Always allow"));
  m_reset->setObjectName(QStringLiteral("chatResetAlways"));
  m_reset->setAutoDefault(false);
  m_reset->setToolTip(tr("Every call that was always allowed asks again"));
  m_reset->setEnabled(anyAlwaysAllowed());
  auto* buttons = new QHBoxLayout;
  buttons->addWidget(m_clear);
  buttons->addWidget(m_reset);
  buttons->addStretch(1);
  form->addRow(QString(), buttons);

  // The API endpoints and local models (phase 2 of mitcad#947): each one
  // offered in the panel's switcher when checked, at its address (https, or
  // http to this computer only), with its key in the keychain.
  for (const ApiAgent::Preset& preset : presets) {
    ApiRow row;
    row.id = preset.id;
    row.preset = preset.url;
    row.enabled = new QCheckBox(preset.name);
    row.enabled->setObjectName(QStringLiteral("chatApiEnabled_") + preset.id);
    row.enabled->setChecked(apiAgentEnabled(preset.id));
    row.enabled->setToolTip(tr("Offer %1 in the chat panel").arg(preset.name));
    row.url = new QLineEdit(apiAgentUrl(preset.id, preset.url));
    row.url->setObjectName(QStringLiteral("chatApiUrl_") + preset.id);
    row.url->setPlaceholderText(preset.url.isEmpty() ? QStringLiteral("https://…/v1") : preset.url);
    row.url->setToolTip(tr("The address of %1: https, or http to this computer only").arg(preset.name));
    auto* line = new QHBoxLayout;
    line->addWidget(row.enabled);
    line->addWidget(row.url, 1);
    if (preset.key != QStringLiteral("none")) {
      row.key = new QLineEdit;
      row.key->setObjectName(QStringLiteral("chatApiKey_") + preset.id);
      row.key->setEchoMode(QLineEdit::Password);
      row.key->setPlaceholderText(tr("New API key"));
      row.key->setToolTip(tr("Kept in the system's keychain, never in a file of MitCad's"));
      auto* remove = new QPushButton(tr("Remove Key"));
      remove->setObjectName(QStringLiteral("chatApiRemoveKey_") + preset.id);
      remove->setAutoDefault(false);
      line->addWidget(row.key, 1);
      line->addWidget(remove);
      const QString id = preset.id;
      connect(remove, &QPushButton::clicked, this, [this, id] {
        removeApiKey(id, this, [id](const QString& error) {
          qInfo().noquote() << QStringLiteral("Preferences: chat's API key of %1 removed%2")
                                   .arg(id, error.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(error));
          emit ChatNotifier::instance().apiSettingsChanged();
        });
      });
      connect(row.key, &QLineEdit::editingFinished, this, &ChatPreferencesBox::changed);
    }
    form->addRow(m_api.empty() ? tr("API endpoints:") : QString(), line);
    connect(row.enabled, &QCheckBox::toggled, this, &ChatPreferencesBox::changed);
    connect(row.url, &QLineEdit::editingFinished, this, &ChatPreferencesBox::changed);
    m_api.push_back(row);
  }
  m_timeout = new QSpinBox;
  m_timeout->setObjectName(QStringLiteral("chatApiTimeout"));
  m_timeout->setRange(10, 3600);
  m_timeout->setSuffix(tr(" s"));
  m_timeout->setValue(apiRequestTimeout());
  m_timeout->setToolTip(tr("The longest wait for the service's next data"));
  form->addRow(tr("Request timeout:"), m_timeout);
  connect(m_timeout, &QSpinBox::valueChanged, this, &ChatPreferencesBox::changed);

  // A custom command that speaks the Agent Client Protocol itself
  // (mitcad#1198): offered in the panel's switcher when checked; its
  // program (a name or a path) and its arguments, on one row.
  m_customEnabled = new QCheckBox(tr("Custom command"));
  m_customEnabled->setObjectName(QStringLiteral("chatCustomEnabled"));
  m_customEnabled->setChecked(customAgentEnabled());
  m_customEnabled->setToolTip(tr("Offer an agent that speaks the Agent Client Protocol in the chat panel"));
  m_customCommand = new QLineEdit(customAgentCommand());
  m_customCommand->setObjectName(QStringLiteral("chatCustomCommand"));
  m_customCommand->setPlaceholderText(tr("Program"));
  m_customCommand->setToolTip(tr("The agent's program: a name found like a command in a terminal, or its path"));
  m_customArguments = new QLineEdit(customAgentArguments());
  m_customArguments->setObjectName(QStringLiteral("chatCustomArguments"));
  m_customArguments->setPlaceholderText(tr("Arguments"));
  m_customArguments->setToolTip(tr("The program's arguments; quote an argument with spaces"));
  auto* custom = new QHBoxLayout;
  custom->addWidget(m_customEnabled);
  custom->addWidget(m_customCommand, 1);
  custom->addWidget(m_customArguments, 1);
  form->addRow(tr("Custom agent:"), custom);
  connect(m_customEnabled, &QCheckBox::toggled, this, &ChatPreferencesBox::changed);
  connect(m_customCommand, &QLineEdit::editingFinished, this, &ChatPreferencesBox::changed);
  connect(m_customArguments, &QLineEdit::editingFinished, this, &ChatPreferencesBox::changed);

  connect(m_agent, &QComboBox::currentIndexChanged, this, &ChatPreferencesBox::changed);
  connect(m_approvals, &QComboBox::currentIndexChanged, this, &ChatPreferencesBox::changed);
  connect(m_history, &QCheckBox::toggled, this, &ChatPreferencesBox::changed);
  connect(m_clear, &QPushButton::clicked, this, [this] {
    if (sheetQuestion(this, tr("Clear All History"),
                      tr("Remove the chat history of every design? The designs do not change."),
                      QMessageBox::Yes | QMessageBox::Cancel, QMessageBox::Cancel) != QMessageBox::Yes) {
      return;
    }
    // The API agents' kept conversations too.
    const bool cleared = ChatHistoryStore(chatDataDirectory()).clearAll() && clearApiSessions();
    qInfo().noquote() << QStringLiteral("Preferences: chat history of every design cleared%1")
                             .arg(cleared ? QString() : QStringLiteral(" (not all of it could be removed)"));
    emit ChatNotifier::instance().historyCleared();
  });
  connect(m_reset, &QPushButton::clicked, this, [this] {
    resetAlwaysAllowed();
    m_reset->setEnabled(false);
    qInfo().noquote() << QStringLiteral("Preferences: chat's Always allow reset");
  });
}

void ChatPreferencesBox::saveApi() {
  bool changed = false;
  const QString chosenAgent = m_agent->currentData().toString();
  for (const ApiRow& row : m_api) {
    // The agent chosen for the chat is offered.
    const bool enabled = row.enabled->isChecked() || row.id == chosenAgent;
    if (enabled != apiAgentEnabled(row.id)) {
      setApiAgentEnabled(row.id, enabled);
      changed = true;
    }
    const QString typed = row.url->text().trimmed();
    const QString address = typed == row.preset ? QString() : typed;
    if (apiAgentUrl(row.id, row.preset) != (address.isEmpty() ? row.preset : address)) {
      setApiAgentUrl(row.id, address);
      changed = true;
    }
    if (row.key != nullptr && !row.key->text().trimmed().isEmpty()) {
      const QString id = row.id;
      saveApiKey(id, row.key->text().trimmed(), &ChatNotifier::instance(), [id](bool keychain, const QString& error) {
        Q_UNUSED(error);
        qInfo().noquote() << QStringLiteral("Preferences: chat's API key of %1 kept %2")
                                 .arg(id, keychain ? QStringLiteral("in the keychain") : QStringLiteral("for this run"));
        emit ChatNotifier::instance().apiSettingsChanged();
      });
      row.key->clear();
    }
  }
  if (m_timeout->value() != apiRequestTimeout()) {
    setApiRequestTimeout(m_timeout->value());
    changed = true;
  }
  // The custom command; offered when it is the agent chosen for the chat.
  bool customChanged = false;
  const bool customOn = m_customEnabled->isChecked() || chosenAgent == QStringLiteral("acp");
  if (customOn != customAgentEnabled()) {
    setCustomAgentEnabled(customOn);
    customChanged = true;
  }
  if (m_customCommand->text().trimmed() != customAgentCommand()) {
    setCustomAgentCommand(m_customCommand->text());
    customChanged = true;
  }
  if (m_customArguments->text().trimmed() != customAgentArguments()) {
    setCustomAgentArguments(m_customArguments->text());
    customChanged = true;
  }
  if (customChanged) {
    qInfo().noquote() << QStringLiteral("Preferences: chat's custom command %1: %2 %3")
                             .arg(customOn ? QStringLiteral("offered") : QStringLiteral("not offered"),
                                  customAgentCommand(), customAgentArguments());
  }
  if (changed) {
    qInfo().noquote() << QStringLiteral("Preferences: chat's API endpoints changed");
  }
  if (changed || customChanged) {
    emit ChatNotifier::instance().apiSettingsChanged();
  }
}

ChatSettings ChatPreferencesBox::chosen() const {
  ChatSettings settings;
  settings.agent = m_agent->currentData().toString();
  settings.approvals = approvalModeFromKey(m_approvals->currentData().toString());
  settings.history = m_history->isChecked();
  return settings;
}

} // namespace mitcad::agent
