// SPDX-License-Identifier: MIT
// What a command panel shows, for live MCP (mitcad#265).
#include "CommandPanel.hpp"

#include <QJsonObject>
#include <QLabel>
#include <QPushButton>
#include <QTextDocumentFragment>

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
  return {{QStringLiteral("message"), m_messageText},
          {QStringLiteral("message_level"), noticeLevelName(m_messageLevel)},
          {QStringLiteral("result"), QTextDocumentFragment::fromHtml(m_result->text()).toPlainText()},
          {QStringLiteral("ok_enabled"), m_ok->isEnabled()},
          {QStringLiteral("notes"), notes}};
}

} // namespace mitcad
