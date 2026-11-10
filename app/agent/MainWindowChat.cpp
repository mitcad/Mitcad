// SPDX-License-Identifier: MIT
// The chat panel in the window (mitcad#947): View › Chat and its button, the
// panel as a card on the right edge of the Floating layout or as a dock of
// the Docked one, and what the conversation needs of the window: the
// document's undo depth and revision, merge_undo, the selection, a picture
// of the view, the live MCP listener, the agent indicator. The AppKit
// frontend shows the same panel in its Chat card (mitcad#1075).
#include "../MainWindow.hpp"

#include <algorithm>

#include <QAction>
#include <QApplication>
#include <QDockWidget>
#include <QEvent>
#include <QFrame>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QLayout>
#include <QMenu>
#include <QResizeEvent>
#include <QScrollArea>
#include <QScrollBar>
#include <QSize>
#include <QStringList>
#include <QTimer>
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
#ifdef MITCAD_MACOS_NATIVE_UI
#include "../platform/NativeDialogs.hpp"
#include "../platform/NativeMacWindow.hpp"
#endif
#include "ChatController.hpp"
#include "ChatPanel.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad {
namespace {

// The chat panel in the Floating card (mitcad#1590). The card is as high as
// the right column, which a short window makes lower than the panel's
// minimum; the panel then keeps its minimum height for the card's width
// and scrolls as a whole instead of being squeezed (a confirmation's text
// and buttons cut). With the room it fills the card as before.
class ChatPanelScroll : public QScrollArea {
public:
  explicit ChatPanelScroll(QWidget* panel) {
    setObjectName(QStringLiteral("chatPanelScroll"));
    setFrameShape(QFrame::NoFrame);
    setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    setWidgetResizable(false);
    setWidget(panel);
  }

  QSize sizeHint() const override { return widget() != nullptr ? widget()->sizeHint() : QScrollArea::sizeHint(); }

  QSize minimumSizeHint() const override {
    return {widget() != nullptr ? widget()->minimumSizeHint().width() : 0, 0};
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    // The panel's contents changed: its minimum may have too.
    if (watched == widget() && event->type() == QEvent::LayoutRequest) {
      place();
    }
    return QScrollArea::eventFilter(watched, event);
  }

  void resizeEvent(QResizeEvent* event) override {
    QScrollArea::resizeEvent(event);
    place();
  }

  bool viewportEvent(QEvent* event) override {
    // A scroll bar shown or hidden changes the view's width.
    if (event->type() == QEvent::Resize) {
      place();
    }
    return QScrollArea::viewportEvent(event);
  }

private:
  // The view's size, or the panel's minimum height at the view's width
  // where that is more; then its lower part in view, where its actions
  // are (the message, Sign in Again's confirmation).
  void place() {
    QWidget* panel = widget();
    if (panel == nullptr) {
      return;
    }
    const int width = viewport()->width();
    int least = std::max(panel->minimumSizeHint().height(), panel->minimumHeight());
    if (QLayout* layout = panel->layout(); layout != nullptr && layout->hasHeightForWidth()) {
      least = std::max(least, layout->minimumHeightForWidth(width));
    }
    const QSize size(width, std::max(viewport()->height(), least));
    if (panel->size() != size) {
      panel->resize(size);
      verticalScrollBar()->setValue(verticalScrollBar()->maximum());
    }
  }
};

} // namespace

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
    setChatShown(m_nativeUi ? !m_nativeChatShown : m_chatContainer->isHidden());
  };
  m_registry->add(chat);
}

mcp::ChatGate* MainWindow::chatGate() const { return m_chat; }

QList<mcp::OutgoingAgent> MainWindow::chatOutgoingAgents() const {
  QList<mcp::OutgoingAgent> outgoing;
  if (m_chat == nullptr) {
    return outgoing;
  }
  for (const agent::ChatAgent* candidate : m_chat->foundAgents()) {
    // "Claude Code (chat panel)", "Claude Code (~/.local/bin, chat panel)".
    const QString name = candidate->place().isEmpty()
                             ? tr("%1 (chat panel)").arg(candidate->name())
                             : tr("%1 (%2, chat panel)").arg(candidate->name(), candidate->place());
    outgoing.append({candidate->id(), name, m_chat->menuState(candidate), agent::ChatController::offered(candidate)});
  }
  return outgoing;
}

void MainWindow::offerChatAgent(const QString& id, bool offered) {
  if (m_chat != nullptr) {
    m_chat->setOffered(id, offered);
  }
}

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
  // The unit of the design's lengths (mitcad#1452), the snapshot's while
  // a job computes the model.
  host.lengthUnit = [this] { return this->lengthUnit(); };
  // The open comments (mitcad#1119), shortened to what the agent needs:
  // the full comment is in mitcad://comments.
  host.comments = [this]() -> QJsonArray {
    if (modelBusy()) {
      return {};
    }
    QJsonArray open;
    try {
      const QJsonArray comments =
          query({{QStringLiteral("query"), QStringLiteral("comments")}, {QStringLiteral("status"), QStringLiteral("open")}})
              .toObject()
              .value(QStringLiteral("comments"))
              .toArray();
      for (const QJsonValue& value : comments) {
        const QJsonObject comment = value.toObject();
        open.append(QJsonObject{{QStringLiteral("id"), comment.value(QStringLiteral("id"))},
                                {QStringLiteral("text"), comment.value(QStringLiteral("text"))},
                                {QStringLiteral("label"), comment.value(QStringLiteral("label"))},
                                {QStringLiteral("author"), comment.value(QStringLiteral("author"))}});
      }
    } catch (const std::exception& error) {
      qInfo().noquote() << QStringLiteral("Chat: no comments: %1").arg(QString::fromUtf8(error.what()));
    }
    return open;
  };
  // The assistants connected to this window (mitcad#1297): the indicator
  // already leaves the chat's own agent out.
  host.assistants = [this]() {
    QStringList labels;
    for (const mcp::ConnectedAgent& connected : m_agentIndicator->agents()) {
      if (!connected.chat) {
        labels << connected.name;
      }
    }
    return labels;
  };
  m_chat = new agent::ChatController(std::move(host), this);

  QAction* action = m_registry->action(QStringLiteral("view.chat"));
  if (action != nullptr) {
    action->setCheckable(true);
  }
  noteChatDocument();
  m_chatPanel = new agent::ChatPanel(*m_chat);
  connect(m_chatPanel, &agent::ChatPanel::connectAssistantRequested, this,
          [this] { trigger(QStringLiteral("help.connect_assistant")); });
  connect(m_chatPanel, &agent::ChatPanel::preferencesRequested, this,
          [this] { trigger(QStringLiteral("tools.preferences.agents")); });
  if (m_nativeUi) {
    // The AppKit window's Chat card presents the panel through the bridge
    // (createNativeChat, once that window is made); its Qt widget stays
    // hidden.
    m_chatPanel->setParent(this);
    m_chatPanel->hide();
    m_chatContainer = m_chatPanel;
  } else if (chromeStyle() == ChromeStyle::Floating && m_floating != nullptr) {
    auto* card = new GlassCard;
    card->setObjectName(QStringLiteral("chatCard"));
    card->setTitle(tr("Chat"));
    // In a scroll area of its own: a short window scrolls the panel instead
    // of squeezing it (mitcad#1590).
    card->setContent(new ChatPanelScroll(m_chatPanel), QMargins(8, 4, 8, 8));
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
      // Named for the tests of the bar's colours (mitcad#1245).
      if (QWidget* button = action != nullptr ? bar->widgetForAction(action) : nullptr) {
        button->setObjectName(QStringLiteral("ribbonToolbarChat"));
      }
    }
  }
  // The AppKit frontend's View menu and toolbar button come with its window
  // (createNativeChat).
  if (QMenu* view = m_nativeUi ? nullptr : findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    view->addAction(action);
  }
  if (auto* titleBar = m_nativeUi ? nullptr : findChild<TitleBar*>()) {
    // Beside the agent indicator in the title bar row.
    auto* button = new CapsuleFrame(-1);
    button->setObjectName(QStringLiteral("titleChat"));
    auto* row = new QHBoxLayout(button);
    row->setContentsMargins(4, 2, 4, 3);
    row->addWidget(new RibbonButton(action, uiIconSize(18)));
    button->setCardMaterial(titleBar->isFloatingRow());
    titleBar->setChatButton(button);
  }
  // Looked for again when MitCad comes to the front: an agent installed or
  // signed in meanwhile is used without a restart.
  connect(qApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    const bool shown = m_nativeUi ? m_nativeChatShown : m_chatContainer != nullptr && !m_chatContainer->isHidden();
    if (state == Qt::ApplicationActive && shown && m_chat->agent()->readiness() != agent::ChatAgent::Readiness::Ready) {
      m_chat->refreshSetup(true);
    }
  });
}

void MainWindow::setChatShown(bool shown) {
  if (m_chatContainer == nullptr) {
    return;
  }
  if (m_nativeUi) {
    // The AppKit Chat card (mitcad#1075): the panel's Qt widget stays
    // hidden; the card shows its native view and gives the message field
    // the keyboard.
    m_nativeChatShown = shown;
    m_chatPanel->setPresented(shown);
#ifdef MITCAD_MACOS_NATIVE_UI
    if (m_nativeWindow) {
      if (m_nativeChatForm) {
        m_nativeChatForm->refresh();
      }
      m_nativeWindow->showChat(shown);
    }
#endif
  } else {
    m_chatContainer->setVisible(shown);
    if (m_floating != nullptr) {
      m_floating->requestLayout();
    }
  }
  if (QAction* action = m_registry->action(QStringLiteral("view.chat"))) {
    action->setChecked(shown);
  }
  qInfo().noquote() << QStringLiteral("Chat %1").arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
  if (shown && m_chatPanel != nullptr && !m_nativeUi) {
    m_chatPanel->focusInput();
  }
}

#ifdef MITCAD_MACOS_NATIVE_UI
void MainWindow::createNativeChat() {
  if (m_chatPanel == nullptr || !m_nativeWindow) {
    return;
  }
  // View › Chat, after the native window's Browser and Timeline: last, as
  // in the Qt frontend's View menu.
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    view->addAction(m_registry->action(QStringLiteral("view.chat")));
  }
  m_nativeChatForm = createNativeWidgetView(m_chatPanel);
  m_nativeWindow->setChatView(m_nativeChatForm ? m_nativeChatForm->nativeView() : nullptr);
  // What the hidden panel changes (rows, streamed answers, its state)
  // reaches the card on the next turn, once for a burst of changes; the
  // card then keeps the conversation's end in sight.
  auto* refresh = new QTimer(this);
  refresh->setSingleShot(true);
  refresh->setInterval(0);
  connect(refresh, &QTimer::timeout, this, [this] {
    if (m_nativeChatForm && m_nativeWindow) {
      m_nativeChatForm->refresh();
      m_nativeWindow->setChatView(m_nativeChatForm->nativeView());
    }
  });
  connect(m_chatPanel, &agent::ChatPanel::contentChanged, refresh, qOverload<>(&QTimer::start));
}
#endif

void MainWindow::noteChatDocument() {
  if (m_chat != nullptr) {
    m_chat->documentChanged(m_mcpDocumentId, m_filePath);
  }
}

} // namespace mitcad
