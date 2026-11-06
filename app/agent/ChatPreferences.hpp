// SPDX-License-Identifier: MIT
#pragma once

// Preferences › AI Assistants › Chat (mitcad#947): the chat's agent, which
// of its calls ask first, whether the history is kept, Clear All History
// and Reset Always Allow; the API endpoints and local models (phase 2):
// which are offered in the panel, each one's address and API key, and the
// requests' timeout; a custom command that speaks the Agent Client Protocol
// (mitcad#1198): whether it is offered in the panel, its command and its
// arguments. The choices are saved when Preferences is accepted (chosen(),
// saveApi(), which saves the custom command too); the buttons act at once.

#include <vector>

#include <QGroupBox>
#include <QString>
#include <QWidget>

#include "ChatSettings.hpp"

class QCheckBox;
class QComboBox;
class QLineEdit;
class QPushButton;
class QSpinBox;

namespace mitcad::agent {

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none (as AgentPreferencesBox).
#ifdef MITCAD_MAC_UI
using ChatPreferencesBase = QWidget;
#else
using ChatPreferencesBase = QGroupBox;
#endif

class ChatPreferencesBox : public ChatPreferencesBase {
  Q_OBJECT

public:
  explicit ChatPreferencesBox(QWidget* parent = nullptr);

  ChatSettings chosen() const;
  // The API agents' and the custom command's choices: saved, and the open
  // panels told, when any changed. A key typed here goes to the keychain.
  void saveApi();

signals:
  void changed();

private:
  QComboBox* m_agent = nullptr;
  QComboBox* m_approvals = nullptr;
  QCheckBox* m_history = nullptr;
  QPushButton* m_clear = nullptr;
  QPushButton* m_reset = nullptr;
  struct ApiRow {
    QString id;
    QString preset; // its preset address
    QCheckBox* enabled = nullptr;
    QLineEdit* url = nullptr;
    QLineEdit* key = nullptr; // none for a service without keys
  };
  std::vector<ApiRow> m_api;
  QSpinBox* m_timeout = nullptr;
  QCheckBox* m_customEnabled = nullptr;
  QLineEdit* m_customCommand = nullptr;
  QLineEdit* m_customArguments = nullptr;
};

} // namespace mitcad::agent
