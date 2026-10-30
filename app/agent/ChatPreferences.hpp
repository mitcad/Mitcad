// SPDX-License-Identifier: MIT
#pragma once

// Preferences › AI Assistants › Chat (mitcad#947): the chat's agent, which
// of its calls ask first, whether the history is kept, Clear All History
// and Reset Always Allow. The choices are saved when Preferences is
// accepted (chosen()); the two buttons act at once.

#include <QGroupBox>
#include <QWidget>

#include "ChatSettings.hpp"

class QCheckBox;
class QComboBox;
class QPushButton;

namespace mitcad::agent {

// A group with a frame and a title, except on macOS, where the settings
// window's panes have none (as AgentPreferencesBox).
#ifdef Q_OS_MACOS
using ChatPreferencesBase = QWidget;
#else
using ChatPreferencesBase = QGroupBox;
#endif

class ChatPreferencesBox : public ChatPreferencesBase {
  Q_OBJECT

public:
  explicit ChatPreferencesBox(QWidget* parent = nullptr);

  ChatSettings chosen() const;

signals:
  void changed();

private:
  QComboBox* m_agent = nullptr;
  QComboBox* m_approvals = nullptr;
  QCheckBox* m_history = nullptr;
  QPushButton* m_clear = nullptr;
  QPushButton* m_reset = nullptr;
};

} // namespace mitcad::agent
