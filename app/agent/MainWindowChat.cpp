// SPDX-License-Identifier: MIT
// The chat panel in the window (mitcad#947): View › Chat and its button, the
// panel as a card on the right edge of the Floating layout or as a dock of
// the Docked one, and what the conversation needs of the window: the
// document's undo depth and revision, merge_undo, the selection, a picture
// of the view, the live MCP listener, the agent indicator.
#include "../MainWindow.hpp"

#include <QAction>
#include <QApplication>
#include <QDockWidget>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QMenu>
#include <QToolBar>
#include <QUrl>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/Command.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Ribbon.hpp"
#include "../framework/TitleBar.hpp"
#include "../mcp/AgentIndicator.hpp"
#include "../report/ReportCenter.hpp"
#include "ChatController.hpp"
#include "ChatPanel.hpp"

namespace mitcad {

void MainWindow::registerChatCommands() {
  CommandDef chat;
  chat.id = QStringLiteral("view.chat");
  chat.name = tr("Chat");
  chat.icon = QStringLiteral("assistant-chat");
  chat.kind = CommandDef::Kind::Action;
  chat.mode = CommandDef::Mode::Any;
  chat.tab.clear(); // the View menu and its button
  chat.tooltip = tr("Asks an AI agent to change the open design");
  chat.keywords = {QStringLiteral("ai"),     QStringLiteral("chat"),   QStringLiteral("claude"),
                   QStringLiteral("agent"),  QStringLiteral("prompt"), QStringLiteral("assistant"),
                   QStringLiteral("claude code"), QStringLiteral("codex"), QStringLiteral("model")};
  chat.duringCommands = true;
  chat.shortcut = QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_L);
  chat.run = [this] {
    if (m_chatContainer == nullptr) {
      showHint(tr("The chat is not in this window's layout yet."));
      return;
    }
    setChatShown(m_chatContainer->isHidden());
  };
  m_registry->add(chat);
}

mcp::ChatGate* MainWindow::chatGate() const { return m_chat; }

void MainWindow::createChat() {
  agent::ChatHost host;
  host.documentState = [this]() -> QJsonObject {
    if (modelBusy()) {
      return {};
    }
    try {
      const QJsonObject document = query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject();
      return {{QStringLiteral("undo_depth"), document.value(QStringLiteral("undo_depth"))},
              {QStringLiteral("revision"), document.value(QStringLiteral("revision"))},
              {QStringLiteral("sketch"), m_mode == Mode::Sketch},
              {QStringLiteral("panel"), m_mode == Mode::Command || !m_session.isNull()}};
    } catch (const std::exception&) {
      return {};
    }
  };
  host.mergeUndo = [this](int depth, const QString& label) {
    const bool merged = runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                                    {QStringLiteral("depth"), depth},
                                    {QStringLiteral("label"), label}});
    updateActions();
    return merged;
  };
  host.selection = [this]() -> QJsonArray {
    try {
      return mcpUiState().value(QStringLiteral("selection")).toArray();
    } catch (const std::exception&) {
      return {};
    }
  };
  host.viewPicture = [this]() -> QByteArray {
    try {
      // As ui_screenshot gives it to an agent.
      return mcpScreenshot({{QStringLiteral("max_size"), 1024}}).value(QStringLiteral("image_base64")).toString().toLatin1();
    } catch (const std::exception& error) {
      qInfo().noquote() << QStringLiteral("Chat: no picture of the view: %1").arg(QString::fromUtf8(error.what()));
      return {};
    }
  };
  host.listener = [this](QString* reason) {
    if (reason != nullptr) {
      *reason = m_agentIndicator->unavailableReason();
    }
    return m_agentIndicator->listenerInstance();
  };
  host.setWorking = [this](const QString& agentName) { m_agentIndicator->setChatWorking(agentName); };
  host.whenIdle = [this](std::function<void()> call) { whenIdle(this, std::move(call)); };
  host.openUrl = [](const QString& url) { openExternalUrl(QUrl(url)); };
  m_chat = new agent::ChatController(std::move(host), this);

  QAction* action = m_registry->action(QStringLiteral("view.chat"));
  if (action != nullptr) {
    action->setCheckable(true);
  }
  noteChatDocument();
  if (m_nativeUi) {
    // The native panel comes with the native window's work; the
    // conversation's approvals still answer the live server.
    return;
  }
  m_chatPanel = new agent::ChatPanel(*m_chat);
  connect(m_chatPanel, &agent::ChatPanel::connectAssistantRequested, this,
          [this] { trigger(QStringLiteral("help.connect_assistant")); });
  connect(m_chatPanel, &agent::ChatPanel::preferencesRequested, this,
          [this] { trigger(QStringLiteral("tools.preferences.agents")); });
  if (chromeStyle() == ChromeStyle::Floating && m_floating != nullptr) {
    auto* card = new GlassCard;
    card->setObjectName(QStringLiteral("chatCard"));
    card->setTitle(tr("Chat"));
    card->setContent(m_chatPanel, QMargins(8, 4, 8, 8));
    m_floating->addCard(card, FloatingLayout::Anchor::Right, {340, {}, 1.0, false});
    m_chatContainer = card;
  } else {
    auto* dock = new QDockWidget(tr("Chat"), this);
    dock->setObjectName(QStringLiteral("chatDock"));
    dock->setWidget(m_chatPanel);
    addDockWidget(Qt::RightDockWidgetArea, dock);
    dock->hide();
    m_chatContainer = dock;
    // Closed with the dock's own button: as View › Chat.
    connect(dock->toggleViewAction(), &QAction::toggled, this, [this](bool on) {
      QAction* chatAction = m_registry->action(QStringLiteral("view.chat"));
      if (isVisible() && chatAction != nullptr && chatAction->isChecked() != on) {
        chatAction->setChecked(on);
      }
    });
    if (auto* bar = findChild<QToolBar*>(QStringLiteral("ribbonToolbar"))) {
      bar->addAction(action);
    }
  }
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    view->addAction(action);
  }
  if (auto* titleBar = findChild<TitleBar*>()) {
    // Beside the agent indicator in the title bar row.
    auto* button = new CapsuleFrame(-1);
    button->setObjectName(QStringLiteral("titleChat"));
    auto* row = new QHBoxLayout(button);
    row->setContentsMargins(4, 2, 4, 3);
    row->addWidget(new RibbonButton(action, 18));
    button->setCardMaterial(titleBar->isFloatingRow());
    titleBar->setChatButton(button);
  }
  // Looked for again when MitCad comes to the front: an agent installed or
  // signed in meanwhile is used without a restart.
  connect(qApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state == Qt::ApplicationActive && m_chatContainer != nullptr && !m_chatContainer->isHidden() &&
        m_chat->agent()->readiness() != agent::ChatAgent::Readiness::Ready) {
      m_chat->refreshSetup(true);
    }
  });
}

void MainWindow::setChatShown(bool shown) {
  if (m_chatContainer == nullptr) {
    return;
  }
  m_chatContainer->setVisible(shown);
  if (m_floating != nullptr) {
    m_floating->requestLayout();
  }
  if (QAction* action = m_registry->action(QStringLiteral("view.chat"))) {
    action->setChecked(shown);
  }
  qInfo().noquote() << QStringLiteral("Chat %1").arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
  if (shown && m_chatPanel != nullptr) {
    m_chatPanel->focusInput();
  }
}

void MainWindow::noteChatDocument() {
  if (m_chat != nullptr) {
    m_chat->documentChanged(m_mcpDocumentId, m_filePath);
  }
}

} // namespace mitcad
