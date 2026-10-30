// SPDX-License-Identifier: MIT
#include "ChatPanel.hpp"

#include <utility>

#include <QAbstractButton>
#include <QComboBox>
#include <QDir>
#include <QEvent>
#include <QFrame>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QScrollBar>
#include <QSet>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QUrl>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Icons.hpp"
#include "../framework/Theme.hpp"
#include "../mcp/ConnectAssistantDialog.hpp"
#include "../report/ReportCenter.hpp"
#include "ChatController.hpp"

namespace mitcad::agent {
namespace {

constexpr int kPollMs = 3000;
constexpr int kRenderMs = 60;

QLabel* wrapped(const QString& objectName) {
  auto* label = new QLabel;
  label->setObjectName(objectName);
  label->setWordWrap(true);
  label->setTextInteractionFlags(Qt::TextSelectableByMouse | Qt::LinksAccessibleByMouse);
  // A long word (a path) does not widen the panel.
  label->setMinimumWidth(1);
  return label;
}

QString role(const QJsonObject& entry) { return entry.value(QStringLiteral("role")).toString(); }

} // namespace

ChatPanel::ChatPanel(ChatController& chat, QWidget* parent) : QWidget(parent), m_chat(chat) {
  setObjectName(QStringLiteral("chatPanel"));
  setMinimumWidth(260);
  auto* column = new QVBoxLayout(this);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(6);

  // The header: the agent (a choice once several are found), New
  // conversation and the panel's menu; below it the model.
  auto* header = new QHBoxLayout;
  header->setSpacing(4);
  m_agentName = new QLabel;
  m_agentName->setObjectName(QStringLiteral("chatAgent"));
  m_agentChoice = new QComboBox;
  m_agentChoice->setObjectName(QStringLiteral("chatAgentChoice"));
  m_agentChoice->setToolTip(tr("The agent; another one starts a new conversation"));
  m_agentChoice->hide();
  m_modelChoice = new QComboBox;
  m_modelChoice->setObjectName(QStringLiteral("chatModelChoice"));
  m_modelChoice->setEditable(true);
  m_modelChoice->setInsertPolicy(QComboBox::NoInsert);
  m_modelChoice->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
  m_modelChoice->setMinimumContentsLength(10);
  m_modelChoice->setToolTip(tr("The model the agent uses; a name can be typed"));
  m_model = new QLabel;
  m_model->setObjectName(QStringLiteral("chatModel"));
  m_model->setEnabled(false); // dimmed
  m_model->setMinimumWidth(1);
  m_new = new QToolButton;
  m_new->setObjectName(QStringLiteral("chatNew"));
  m_new->setText(tr("New conversation"));
  m_new->setIcon(themeIcon(QStringLiteral("add")));
  m_new->setToolButtonStyle(Qt::ToolButtonIconOnly);
  m_new->setToolTip(tr("New conversation"));
  m_new->setAutoRaise(true);
  m_more = new QToolButton;
  m_more->setObjectName(QStringLiteral("chatMore"));
  m_more->setText(QStringLiteral("…"));
  m_more->setToolTip(tr("More"));
  m_more->setAutoRaise(true);
  m_more->setPopupMode(QToolButton::InstantPopup);
  auto* menu = new QMenu(m_more);
  menu->addAction(tr("New Conversation"), this, [this] { m_chat.newConversation(); });
  menu->addAction(tr("Clear History of This Design"), this, [this] { m_chat.clearHistory(false); });
  menu->addSeparator();
  menu->addAction(tr("Chat Preferences…"), this, [this] { emit preferencesRequested(); });
  m_more->setMenu(menu);
  header->addWidget(m_agentName);
  header->addWidget(m_agentChoice);
  header->addStretch(1);
  header->addWidget(m_new);
  header->addWidget(m_more);
  column->addLayout(header);
  auto* modelRow = new QHBoxLayout;
  modelRow->setSpacing(4);
  auto* modelLabel = new QLabel(tr("Model:"));
  modelLabel->setBuddy(m_modelChoice);
  modelRow->addWidget(modelLabel);
  modelRow->addWidget(m_modelChoice);
  modelRow->addWidget(m_model, 1);
  column->addLayout(modelRow);
  connect(m_new, &QToolButton::clicked, this, [this] { m_chat.newConversation(); });
  connect(m_agentChoice, &QComboBox::activated, this,
          [this](int index) { m_chat.chooseAgent(m_agentChoice->itemData(index).toString()); });
  connect(m_modelChoice, &QComboBox::activated, this, [this](int) { applyModel(); });
  connect(m_modelChoice->lineEdit(), &QLineEdit::editingFinished, this, [this] { applyModel(); });

  // The conversation.
  m_scroll = new QScrollArea;
  m_scroll->setObjectName(QStringLiteral("chatConversation"));
  m_scroll->setWidgetResizable(true);
  m_scroll->setFrameShape(QFrame::NoFrame);
  m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  m_list = new QWidget;
  m_list->setObjectName(QStringLiteral("chatList"));
  m_listLayout = new QVBoxLayout(m_list);
  m_listLayout->setContentsMargins(0, 0, 4, 0);
  m_listLayout->setSpacing(8);
  m_listLayout->addStretch(1);
  m_scroll->setWidget(m_list);
  column->addWidget(m_scroll, 1);

  m_state = new QLabel;
  m_state->setObjectName(QStringLiteral("chatState"));
  m_state->setWordWrap(true);
  m_state->setTextFormat(Qt::PlainText);
  column->addWidget(m_state);

  // The privacy notice, once per agent, before its first message.
  m_notice = new QFrame;
  m_notice->setObjectName(QStringLiteral("chatPrivacy"));
  m_notice->setFrameShape(QFrame::StyledPanel);
  auto* noticeColumn = new QVBoxLayout(m_notice);
  m_noticeText = wrapped(QStringLiteral("chatPrivacyText"));
  noticeColumn->addWidget(m_noticeText);
  auto* noticeButtons = new QHBoxLayout;
  noticeButtons->addStretch(1);
  auto* cancel = new QPushButton(tr("Cancel"));
  cancel->setObjectName(QStringLiteral("chatCancel"));
  auto* proceed = new QPushButton(tr("Continue"));
  proceed->setObjectName(QStringLiteral("chatContinue"));
  proceed->setDefault(true);
  noticeButtons->addWidget(cancel);
  noticeButtons->addWidget(proceed);
  noticeColumn->addLayout(noticeButtons);
  m_notice->hide();
  column->addWidget(m_notice);
  connect(cancel, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Chat: privacy notice cancelled");
    m_notice->hide();
    m_input->setPlainText(m_waiting);
    m_waiting.clear();
    updateState();
  });
  connect(proceed, &QPushButton::clicked, this, [this] {
    m_chat.acceptPrivacyNotice();
    m_notice->hide();
    const QString text = m_waiting;
    m_waiting.clear();
    send(text);
  });

  // The setup's one action.
  m_setupRow = new QWidget;
  auto* setupLine = new QHBoxLayout(m_setupRow);
  setupLine->setContentsMargins(0, 0, 0, 0);
  m_setup = new QPushButton;
  m_setup->setObjectName(QStringLiteral("chatSetup"));
  m_secondary = new QPushButton(tr("Connect AI Assistant…"));
  m_secondary->setObjectName(QStringLiteral("chatConnectAssistant"));
  m_secondary->setFlat(true);
  setupLine->addWidget(m_setup);
  setupLine->addWidget(m_secondary);
  setupLine->addStretch(1);
  column->addWidget(m_setupRow);
  connect(m_setup, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Chat: %1").arg(m_setup->text());
    if (m_setupAction == QStringLiteral("install")) {
      m_chat.openInstallPage();
    } else if (m_setupAction == QStringLiteral("signin")) {
      m_chat.signIn();
    } else if (m_setupAction == QStringLiteral("check")) {
      m_chat.refreshSetup(true);
    }
  });
  connect(m_secondary, &QPushButton::clicked, this, [this] { emit connectAssistantRequested(); });

  // The message, what goes with it, Send.
  m_inputRow = new QWidget;
  auto* inputColumn = new QVBoxLayout(m_inputRow);
  inputColumn->setContentsMargins(0, 0, 0, 0);
  inputColumn->setSpacing(4);
  m_input = new QPlainTextEdit;
  m_input->setObjectName(QStringLiteral("chatInput"));
  m_input->setPlaceholderText(tr("Describe a change to the design"));
  m_input->setTabChangesFocus(true);
  m_input->setFixedHeight(m_input->fontMetrics().lineSpacing() * 4 + 12);
  m_input->installEventFilter(this);
  inputColumn->addWidget(m_input);
  auto* controls = new QHBoxLayout;
  controls->setSpacing(4);
  m_attachSelection = new QToolButton;
  m_attachSelection->setObjectName(QStringLiteral("chatAttachSelection"));
  m_attachSelection->setText(tr("Selection"));
  m_attachSelection->setToolTip(tr("The selected items go with the message"));
  m_attachSelection->setCheckable(true);
  m_attachSelection->setChecked(true);
  m_attachView = new QToolButton;
  m_attachView->setObjectName(QStringLiteral("chatAttachView"));
  m_attachView->setText(tr("View"));
  m_attachView->setToolTip(tr("A picture of the view goes with the message"));
  m_attachView->setCheckable(true);
  m_send = new QPushButton(tr("Send"));
  m_send->setObjectName(QStringLiteral("chatSend"));
  m_send->setDefault(true);
  controls->addWidget(m_attachSelection);
  controls->addWidget(m_attachView);
  controls->addStretch(1);
  controls->addWidget(m_send);
  inputColumn->addLayout(controls);
  column->addWidget(m_inputRow);
  connect(m_send, &QPushButton::clicked, this, [this] {
    if (m_send->property("chatAction").toString() == QStringLiteral("stop")) {
      m_chat.stop();
    } else {
      submit();
    }
  });
  for (QToolButton* toggle : {m_attachSelection, m_attachView}) {
    connect(toggle, &QToolButton::toggled, this, [toggle](bool on) {
      qInfo().noquote() << QStringLiteral("Chat: %1 %2").arg(toggle->text(), on ? QStringLiteral("attached")
                                                                                : QStringLiteral("not attached"));
    });
  }

  m_poll = new QTimer(this);
  m_poll->setInterval(kPollMs);
  connect(m_poll, &QTimer::timeout, this, [this] { m_chat.refreshSetup(false); });
  m_render = new QTimer(this);
  m_render->setSingleShot(true);
  m_render->setInterval(kRenderMs);
  connect(m_render, &QTimer::timeout, this, [this] {
    const QSet<int> dirty = std::exchange(m_dirty, {});
    for (const int index : dirty) {
      updateRow(index);
    }
  });

  connect(&m_chat, &ChatController::setupChanged, this, &ChatPanel::updateState);
  connect(&m_chat, &ChatController::activityChanged, this, &ChatPanel::updateState);
  connect(&m_chat, &ChatController::conversationReset, this, &ChatPanel::rebuild);
  connect(&m_chat, &ChatController::entryAdded, this, &ChatPanel::addRow);
  connect(&m_chat, &ChatController::entryChanged, this, [this](int index) {
    if (index >= 0 && index < m_chat.entries().size() && role(m_chat.entries()[index]) == QStringLiteral("assistant")) {
      // Streamed: drawn a few times a second.
      m_dirty.insert(index);
      if (!m_render->isActive()) {
        m_render->start();
      }
    } else {
      updateRow(index);
    }
  });
  rebuild();
}

void ChatPanel::focusInput() {
  if (m_inputRow->isVisible()) {
    m_input->setFocus(Qt::OtherFocusReason);
  }
}

void ChatPanel::showEvent(QShowEvent* event) {
  QWidget::showEvent(event);
  if (!m_checked) {
    // Opened: the agent looked for (its program, version, sign-in).
    m_checked = true;
    m_chat.refreshSetup(true);
  }
  updateState();
}

void ChatPanel::hideEvent(QHideEvent* event) {
  QWidget::hideEvent(event);
  m_poll->stop();
}

bool ChatPanel::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_input && event->type() == QEvent::KeyPress) {
    const auto* key = static_cast<QKeyEvent*>(event);
    if ((key->key() == Qt::Key_Return || key->key() == Qt::Key_Enter) && !(key->modifiers() & Qt::ShiftModifier)) {
      submit();
      return true;
    }
    if (key->key() == Qt::Key_Escape) {
      if (m_chat.activity() == ChatController::Activity::Working ||
          m_chat.activity() == ChatController::Activity::Starting) {
        m_chat.stop();
        return true;
      }
    }
  }
  return QWidget::eventFilter(watched, event);
}

void ChatPanel::submit() {
  const QString text = m_input->toPlainText().trimmed();
  if (text.isEmpty() || !m_inputRow->isVisible() || m_notice->isVisible()) {
    return;
  }
  m_input->clear();
  if (m_chat.needsPrivacyNotice()) {
    m_waiting = text;
    m_noticeText->setText(tr("What you write here, the selection and pictures of the view go to %1's service, "
                             "whose terms apply. The agent changes this design through MitCad's tools.")
                              .arg(m_chat.agent()->name()));
    m_notice->show();
    updateState();
    return;
  }
  send(text);
}

void ChatPanel::send(const QString& text) {
  if (!text.isEmpty()) {
    m_chat.send(text, m_attachSelection->isChecked(), m_attachView->isChecked());
  }
  updateState();
  m_input->setFocus(Qt::OtherFocusReason);
}

void ChatPanel::rebuild() {
  for (QWidget* row : std::as_const(m_rows)) {
    if (row != nullptr) {
      m_listLayout->removeWidget(row);
      row->hide();
      row->deleteLater();
    }
  }
  m_rows.clear();
  m_dirty.clear();
  for (int index = 0; index < m_chat.entries().size(); ++index) {
    addRow(index);
  }
  updateState();
  keepAtEnd(true);
}

QWidget* ChatPanel::makeRow(const QJsonObject& entry) {
  const QString kind = role(entry);
  if (kind == QStringLiteral("tool")) {
    // A row that opens to the call's input and result.
    auto* row = new QWidget;
    row->setObjectName(QStringLiteral("chatTool"));
    auto* rowColumn = new QVBoxLayout(row);
    rowColumn->setContentsMargins(0, 0, 0, 0);
    rowColumn->setSpacing(2);
    auto* head = new QToolButton;
    head->setObjectName(QStringLiteral("chatToolHead"));
    head->setCheckable(true);
    head->setAutoRaise(true);
    head->setArrowType(Qt::RightArrow);
    head->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    // Its text is cut, not the panel widened (the tooltip has all of it).
    head->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Fixed);
    auto* details = wrapped(QStringLiteral("chatToolDetails"));
    details->setTextFormat(Qt::PlainText);
    details->hide();
    rowColumn->addWidget(head);
    rowColumn->addWidget(details);
    connect(head, &QToolButton::toggled, row, [head, details](bool open) {
      head->setArrowType(open ? Qt::DownArrow : Qt::RightArrow);
      details->setVisible(open);
    });
    return row;
  }
  if (kind == QStringLiteral("card")) {
    // An approval: the call waits for the answer.
    auto* card = new QFrame;
    card->setObjectName(QStringLiteral("chatApproval"));
    card->setFrameShape(QFrame::StyledPanel);
    auto* cardColumn = new QVBoxLayout(card);
    cardColumn->addWidget(wrapped(QStringLiteral("chatApprovalText")));
    auto* buttons = new QWidget;
    buttons->setObjectName(QStringLiteral("chatApprovalButtons"));
    auto* line = new QHBoxLayout(buttons);
    line->setContentsMargins(0, 0, 0, 0);
    const QString id = entry.value(QStringLiteral("id")).toString();
    const struct {
      const char* name;
      const char* text;
      ChatController::Answer answer;
    } choices[] = {{"chatAllowOnce", QT_TR_NOOP("Allow once"), ChatController::Answer::Once},
                   {"chatAlwaysAllow", QT_TR_NOOP("Always allow"), ChatController::Answer::Always},
                   {"chatDeny", QT_TR_NOOP("Deny"), ChatController::Answer::Deny}};
    for (const auto& choice : choices) {
      auto* button = new QPushButton(tr(choice.text));
      button->setObjectName(QLatin1String(choice.name));
      const ChatController::Answer answer = choice.answer;
      connect(button, &QPushButton::clicked, this, [this, id, answer] { m_chat.answer(id, answer); });
      line->addWidget(button);
    }
    line->addStretch(1);
    cardColumn->addWidget(buttons);
    return card;
  }
  QLabel* label = wrapped(kind == QStringLiteral("user")        ? QStringLiteral("chatUser")
                          : kind == QStringLiteral("assistant") ? QStringLiteral("chatAnswer")
                                                                : QStringLiteral("chatNotice"));
  if (kind == QStringLiteral("assistant")) {
    label->setTextFormat(Qt::MarkdownText);
    label->setOpenExternalLinks(false);
    connect(label, &QLabel::linkActivated, this, [](const QString& link) { openExternalUrl(QUrl(link)); });
  } else {
    label->setTextFormat(Qt::PlainText);
  }
  if (kind == QStringLiteral("user")) {
    label->setStyleSheet(QStringLiteral("QLabel { background: palette(alternate-base); border-radius: 8px; "
                                        "padding: 6px 8px; }"));
  } else if (kind != QStringLiteral("assistant")) {
    label->setEnabled(false); // a notice: dimmed
  }
  return label;
}

QString ChatPanel::cardText(const QJsonObject& entry) const {
  const QString kind = entry.value(QStringLiteral("kind")).toString();
  const QString path = QDir::toNativeSeparators(entry.value(QStringLiteral("path")).toString());
  const QString agent = m_chat.agent()->name();
  if (kind == QStringLiteral("document_save")) {
    return path.isEmpty() ? tr("%1 wants to save the design.").arg(agent)
                          : tr("%1 wants to save the design as %2.").arg(agent, path);
  }
  if (kind == QStringLiteral("document_open")) {
    return tr("%1 wants to open %2 in place of this design.").arg(agent, path);
  }
  if (kind == QStringLiteral("model_export")) {
    return tr("%1 wants to export to %2.").arg(agent, path);
  }
  if (kind == QStringLiteral("screenshot_file")) {
    return tr("%1 wants to save a picture of the view as %2.").arg(agent, path);
  }
  if (kind == QStringLiteral("file_command")) {
    return tr("%1 wants to run a File command.").arg(agent);
  }
  if (kind == QStringLiteral("undo_past_start")) {
    return tr("%1 wants to undo a change made before this message.").arg(agent);
  }
  if (kind == QStringLiteral("edit")) {
    return tr("%1 wants to change the design.").arg(agent);
  }
  return tr("%1 wants to use %2.").arg(agent, entry.value(QStringLiteral("tool")).toString());
}

void ChatPanel::fillRow(QWidget* row, const QJsonObject& entry) {
  const QString kind = role(entry);
  if (kind == QStringLiteral("tool")) {
    auto* head = row->findChild<QToolButton*>(QStringLiteral("chatToolHead"));
    auto* details = row->findChild<QLabel*>(QStringLiteral("chatToolDetails"));
    const QString state = entry.value(QStringLiteral("state")).toString();
    const QString summary = entry.value(QStringLiteral("summary")).toString();
    const QString mark = state == QStringLiteral("failed")  ? tr("failed")
                         : state == QStringLiteral("done") ? QString()
                                                           : tr("running");
    QString text = entry.value(QStringLiteral("name")).toString();
    if (!summary.isEmpty()) {
      text += QStringLiteral(" · ") + summary;
    }
    if (!mark.isEmpty()) {
      text += QStringLiteral(" — ") + mark;
    }
    head->setText(text);
    head->setToolTip(text);
    row->setProperty("toolState", state);
    if (state == QStringLiteral("failed")) {
      setErrorStyleSheet(head);
    }
    QStringList lines;
    const QString input = entry.value(QStringLiteral("input")).toString();
    if (!input.isEmpty()) {
      lines << tr("Input: %1").arg(input);
    }
    const QString result = entry.value(QStringLiteral("result")).toString();
    if (!result.isEmpty()) {
      lines << tr("Result: %1").arg(result);
    }
    details->setText(lines.join(QLatin1Char('\n')));
    return;
  }
  if (kind == QStringLiteral("card")) {
    auto* text = row->findChild<QLabel*>(QStringLiteral("chatApprovalText"));
    auto* buttons = row->findChild<QWidget*>(QStringLiteral("chatApprovalButtons"));
    const QString state = entry.value(QStringLiteral("state")).toString();
    QString line = cardText(entry);
    if (state == QStringLiteral("allowed")) {
      line += QLatin1Char(' ') + tr("Allowed once.");
    } else if (state == QStringLiteral("always")) {
      line += QLatin1Char(' ') + tr("Always allowed.");
    } else if (state == QStringLiteral("denied")) {
      line += QLatin1Char(' ') + tr("Denied.");
    } else if (state == QStringLiteral("gone")) {
      line += QLatin1Char(' ') + tr("No longer asked.");
    }
    text->setText(line);
    buttons->setVisible(state == QStringLiteral("open"));
    row->setProperty("cardState", state);
    return;
  }
  auto* label = static_cast<QLabel*>(row);
  label->setText(entry.value(QStringLiteral("text")).toString());
}

void ChatPanel::addRow(int index) {
  if (index < 0 || index >= m_chat.entries().size()) {
    return;
  }
  const QJsonObject& entry = m_chat.entries()[index];
  QWidget* row = makeRow(entry);
  fillRow(row, entry);
  // Before the stretch at the end.
  m_listLayout->insertWidget(m_listLayout->count() - 1, row);
  while (m_rows.size() < index) {
    m_rows.append(nullptr);
  }
  if (m_rows.size() == index) {
    m_rows.append(row);
  } else {
    m_rows[index] = row;
  }
  if (role(entry) == QStringLiteral("card")) {
    qInfo().noquote() << QStringLiteral("Chat panel: approval card '%1'").arg(cardText(entry));
  }
  keepAtEnd(role(entry) == QStringLiteral("user") || role(entry) == QStringLiteral("card"));
}

void ChatPanel::updateRow(int index) {
  if (index < 0 || index >= m_rows.size() || index >= m_chat.entries().size() || m_rows[index] == nullptr) {
    return;
  }
  fillRow(m_rows[index], m_chat.entries()[index]);
  keepAtEnd(false);
}

void ChatPanel::keepAtEnd(bool force) {
  QScrollBar* bar = m_scroll->verticalScrollBar();
  const bool atEnd = bar->value() >= bar->maximum() - 8;
  if (force || atEnd) {
    // Once the new row is laid out.
    QTimer::singleShot(0, this, [this] {
      QScrollBar* scroll = m_scroll->verticalScrollBar();
      scroll->setValue(scroll->maximum());
    });
  }
}

void ChatPanel::updateChoices() {
  // The switcher: the agents found, rebuilt only when they change.
  QStringList ids;
  const QList<ChatAgent*> found = m_chat.detectedAgents();
  for (ChatAgent* candidate : found) {
    ids << candidate->id();
  }
  if (ids != m_agentItems) {
    const QSignalBlocker blockAgents(m_agentChoice);
    m_agentChoice->clear();
    for (ChatAgent* candidate : found) {
      m_agentChoice->addItem(candidate->name(), candidate->id());
    }
    m_agentItems = ids;
  }
  const int chosenAgent = m_agentChoice->findData(m_chat.agent()->id());
  if (chosenAgent >= 0 && m_agentChoice->currentIndex() != chosenAgent) {
    const QSignalBlocker blockAgents(m_agentChoice);
    m_agentChoice->setCurrentIndex(chosenAgent);
  }
  m_agentChoice->setVisible(ids.size() > 1);
  m_agentName->setVisible(ids.size() <= 1);

  // The models: the default, the ones the agent names, a typed name.
  const QList<ChatAgent::Model> models = m_chat.agent()->models();
  QString signature = m_chat.agent()->id();
  for (const ChatAgent::Model& model : models) {
    signature += QLatin1Char('\n') + model.id + QLatin1Char('\t') + model.name;
  }
  if (signature != m_modelItems) {
    const QSignalBlocker blockModels(m_modelChoice);
    m_modelChoice->clear();
    m_modelChoice->addItem(tr("Default model"), QString());
    for (const ChatAgent::Model& model : models) {
      m_modelChoice->addItem(model.name.isEmpty() ? model.id : model.name, model.id);
      m_modelChoice->setItemData(m_modelChoice->count() - 1, model.id, Qt::ToolTipRole);
    }
    m_modelItems = signature;
  }
  if (!m_modelChoice->lineEdit()->hasFocus()) {
    const QString chosenModel = m_chat.chosenModel();
    const QSignalBlocker blockModels(m_modelChoice);
    const int at = m_modelChoice->findData(chosenModel);
    if (at >= 0) {
      if (m_modelChoice->currentIndex() != at || m_modelChoice->currentText() != m_modelChoice->itemText(at)) {
        m_modelChoice->setCurrentIndex(at);
      }
    } else if (m_modelChoice->currentText() != chosenModel) {
      m_modelChoice->setEditText(chosenModel);
    }
  }
}

void ChatPanel::applyModel() {
  const QString text = m_modelChoice->currentText().trimmed();
  QString model = text;
  const int index = m_modelChoice->currentIndex();
  if (index >= 0 && m_modelChoice->itemText(index) == text) {
    model = m_modelChoice->itemData(index).toString();
  } else {
    const int named = m_modelChoice->findText(text, Qt::MatchFixedString);
    if (named >= 0) {
      model = m_modelChoice->itemData(named).toString();
    }
  }
  m_chat.setModel(model);
}

void ChatPanel::updateState() {
  ChatAgent* agent = m_chat.agent();
  const QString name = agent->name();
  m_agentName->setText(name);
  updateChoices();
  m_model->setText(m_chat.model());
  m_model->setToolTip(m_chat.model().isEmpty() ? QString() : tr("The model %1 reported").arg(name));
  const QString unavailable = m_chat.unavailable();
  const ChatAgent::Readiness readiness = agent->readiness();
  const ChatController::Activity activity = m_chat.activity();
  QString key;
  QString text;
  QString action;
  QString actionText;
  bool secondary = false;
  if (!unavailable.isEmpty()) {
    key = QStringLiteral("unavailable");
    text = unavailable;
  } else if (m_notice->isVisible()) {
    key = QStringLiteral("notice");
    text = tr("Read this once before the first message.");
  } else if (readiness == ChatAgent::Readiness::Ready || activity != ChatController::Activity::Idle) {
    switch (activity) {
    case ChatController::Activity::Idle:
      key = QStringLiteral("ready");
      text = m_chat.model().isEmpty() ? tr("%1 is ready.").arg(name) : tr("%1 is ready (%2).").arg(name, m_chat.model());
      break;
    case ChatController::Activity::Starting:
      key = QStringLiteral("starting");
      text = tr("Starting %1…").arg(name);
      break;
    case ChatController::Activity::Working:
      key = QStringLiteral("working");
      text = m_chat.queued() > 0 ? tr("%1 is working; %n message(s) waiting.", nullptr, m_chat.queued()).arg(name)
                                 : tr("%1 is working.").arg(name);
      break;
    case ChatController::Activity::Stopping:
      key = QStringLiteral("stopping");
      text = tr("Stopping…");
      break;
    }
  } else {
    switch (readiness) {
    case ChatAgent::Readiness::Checking:
      key = QStringLiteral("checking");
      text = tr("Looking for %1…").arg(name);
      break;
    case ChatAgent::Readiness::Missing:
      key = QStringLiteral("missing");
      secondary = agent->id() == QStringLiteral("claude") && mcp::claudeDesktopOnComputer();
      text = !agent->readinessDetail().isEmpty() ? agent->readinessDetail()
             : secondary ? tr("The chat uses %1; Claude Desktop connects through Connect AI Assistant.").arg(name)
                         : tr("The chat uses %1, which is not installed.").arg(name);
      action = QStringLiteral("install");
      actionText = tr("Install %1").arg(name);
      break;
    case ChatAgent::Readiness::Outdated:
      key = QStringLiteral("outdated");
      text = agent->readinessDetail();
      action = QStringLiteral("install");
      actionText = tr("Update %1").arg(name);
      break;
    case ChatAgent::Readiness::SignedOut:
      key = QStringLiteral("signed-out");
      text = tr("%1 is not signed in.").arg(name);
      action = QStringLiteral("signin");
      actionText = tr("Sign in to %1").arg(agent->accountName());
      break;
    case ChatAgent::Readiness::SigningIn:
      key = QStringLiteral("signing-in");
      text = agent->readinessDetail().isEmpty()
                 ? tr("Finish signing in to %1 in your browser.").arg(agent->accountName())
                 : tr("Finish signing in to %1 in your browser: %2").arg(agent->accountName(), agent->readinessDetail());
      action = QStringLiteral("check");
      actionText = tr("Check again");
      break;
    case ChatAgent::Readiness::Ready:
      break;
    }
  }
  m_state->setText(text);
  m_setupAction = action;
  m_setup->setText(actionText);
  m_setupRow->setVisible(!action.isEmpty());
  m_secondary->setVisible(secondary);
  const bool ready = unavailable.isEmpty() && (readiness == ChatAgent::Readiness::Ready ||
                                                activity != ChatController::Activity::Idle);
  m_inputRow->setVisible(ready && !m_notice->isVisible());
  const bool working = activity == ChatController::Activity::Working ||
                       activity == ChatController::Activity::Starting ||
                       activity == ChatController::Activity::Stopping;
  m_send->setText(working ? tr("Stop") : tr("Send"));
  m_send->setProperty("chatAction", working ? QStringLiteral("stop") : QStringLiteral("send"));
  m_send->setEnabled(activity != ChatController::Activity::Stopping);
  m_new->setEnabled(activity == ChatController::Activity::Idle);
  m_agentChoice->setEnabled(activity == ChatController::Activity::Idle);
  m_modelChoice->setEnabled(activity == ChatController::Activity::Idle);
  // A missing agent is looked for while the panel shows: installed, it is
  // used without a restart.
  if (readiness == ChatAgent::Readiness::Missing && isVisible()) {
    if (!m_poll->isActive()) {
      m_poll->start();
    }
  } else {
    m_poll->stop();
  }
  setProperty("chatState", key);
  if (key != m_loggedState) {
    m_loggedState = key;
    qInfo().noquote() << QStringLiteral("Chat panel: %1").arg(key);
  }
}

} // namespace mitcad::agent
