// SPDX-License-Identifier: MIT
// What a command panel shows, for live MCP (mitcad#265).
#include "CommandPanel.hpp"

#include <QJsonObject>
#include <QLabel>
#include <QPushButton>
#include <QTextDocumentFragment>

#include "BusyMark.hpp"
#include "CommandMessage.hpp"

namespace mitcad {

QJsonObject CommandPanel::shownState() const {
  QJsonObject notes;
  for (auto it = m_notes.cbegin(); it != m_notes.cend(); ++it) {
    if (!it.value().first.isEmpty()) {
      notes.insert(it.key(), QJsonObject{{QStringLiteral("error"), it.value().first}});
    } else if (!it.value().second.isEmpty()) {
      notes.insert(it.key(), QJsonObject{{QStringLiteral("hint"), it.value().second}});
    }
  }
  QJsonObject shown{{QStringLiteral("message"), m_messageText},
                    {QStringLiteral("message_level"), noticeLevelName(m_messageLevel)},
                    {QStringLiteral("result"), QTextDocumentFragment::fromHtml(m_result->text()).toPlainText()},
                    {QStringLiteral("ok_enabled"), m_ok->isEnabled()},
                    {QStringLiteral("notes"), notes}};
  // The button beside the message while it shows (mitcad#1634).
  if (!m_messageActionText.isEmpty()) {
    shown.insert(QStringLiteral("message_action"), m_messageActionText);
  }
  // Work under way, such as a preview being computed (mitcad#1907), whether
  // its busy mark shows yet or not; it is no longer the message.
  if (m_busy->busy()) {
    shown.insert(QStringLiteral("busy"), m_busy->toolTip());
  }
  return shown;
}

} // namespace mitcad
