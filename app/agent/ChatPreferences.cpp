// SPDX-License-Identifier: MIT
#include "ChatPreferences.hpp"

#include <algorithm>

#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QMessageBox>
#include <QPushButton>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "ChatHistoryStore.hpp"

namespace mitcad::agent {

ChatPreferencesBox::ChatPreferencesBox(QWidget* parent)
#ifdef Q_OS_MACOS
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

  connect(m_agent, &QComboBox::currentIndexChanged, this, &ChatPreferencesBox::changed);
  connect(m_approvals, &QComboBox::currentIndexChanged, this, &ChatPreferencesBox::changed);
  connect(m_history, &QCheckBox::toggled, this, &ChatPreferencesBox::changed);
  connect(m_clear, &QPushButton::clicked, this, [this] {
    if (sheetQuestion(this, tr("Clear All History"),
                      tr("Remove the chat history of every design? The designs do not change."),
                      QMessageBox::Yes | QMessageBox::Cancel, QMessageBox::Cancel) != QMessageBox::Yes) {
      return;
    }
    const bool cleared = ChatHistoryStore(chatDataDirectory()).clearAll();
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

ChatSettings ChatPreferencesBox::chosen() const {
  ChatSettings settings;
  settings.agent = m_agent->currentData().toString();
  settings.approvals = approvalModeFromKey(m_approvals->currentData().toString());
  settings.history = m_history->isChecked();
  return settings;
}

} // namespace mitcad::agent
