// SPDX-License-Identifier: MIT
#include "AgentIndicator.hpp"

#include <algorithm>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QCoreApplication>
#include <QEvent>
#include <QIcon>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QStringList>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "AssistantSetup.hpp"
#include "LocalEndpoint.hpp"

namespace mitcad::mcp {
namespace {

// The menu's arrow at the right end of the Floating capsule.
constexpr int kArrowWidth = 28;
constexpr qreal kDot = 8.0;

// "14:02" today, else "2026-10-05 14:02".
QString shortTime(const QDateTime& time) {
  const QDateTime local = time.toLocalTime();
  return local.date() == QDate::currentDate() ? local.toString(QStringLiteral("HH:mm"))
                                              : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

// "Claude Code, connected since 14:02": the assistant's name; the client's
// own label and version are in its row's tooltip (mitcad#1299).
QString agentLine(const ConnectedAgent& agent) {
  return AgentIndicator::tr("%1, connected since %2").arg(assistantDisplayName(agent.name), shortTime(agent.since));
}

// "claude-ai 0.1.0": what the client called itself.
QString ownLabel(const ConnectedAgent& agent) {
  return agent.version.isEmpty() ? agent.name : QString(agent.name + QLatin1Char(' ') + agent.version);
}

// A menu entry as the log shows it: a checked row with "✓ ", its state after
// " — ".
QString loggedEntry(const QAction* action) {
  QString text = action->text().remove(QLatin1Char('&')).replace(QLatin1Char('\t'), QStringLiteral(" — "));
  return action->isChecked() ? QStringLiteral("✓ ") + text : text;
}

} // namespace

AgentIndicator::AgentIndicator(Host host, QWidget* parent) : QToolButton(parent), m_host(std::move(host)) {
  setObjectName(QStringLiteral("agentIndicator"));
  setAutoRaise(true);
  setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  setFocusPolicy(Qt::NoFocus);
  setAttribute(Qt::WA_Hover);
  if (chromeStyle() == ChromeStyle::Floating) {
    // The title row may narrow it, down to its dot and arrow.
    setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
  }
  m_menu = new QMenu(this);
  // The rows' tooltips name the clients' own labels (mitcad#1299).
  m_menu->setToolTipsVisible(true);
  setMenu(m_menu);
  // A row blocked or allowed here, in another window or in Connect AI
  // Assistant: "· 1 blocked" follows.
  watchAssistantBlocks(this, [this] { refresh(); });
  connect(m_menu, &QMenu::aboutToShow, this, &AgentIndicator::fillMenu);
  // With no assistant connected the button itself connects one; the arrow
  // opens the menu (MenuButtonPopup). Connected or unavailable, it is all
  // menu.
  connect(this, &QToolButton::clicked, this, [this] {
    if (state() == State::NoAgent) {
      openConnect();
    }
  });
  setContextMenuPolicy(Qt::CustomContextMenu);
  connect(this, &QWidget::customContextMenuRequested, this,
          [this](const QPoint& point) { m_menu->popup(mapToGlobal(point)); });
  refresh();
}

void AgentIndicator::setListener(const QString& instance, int port) {
  m_instance = instance;
  m_port = port;
  m_unavailable = false;
  m_reason.clear();
  refresh();
}

void AgentIndicator::setUnavailable(const QString& reason) {
  m_instance.clear();
  m_port = 0;
  m_unavailable = true;
  m_reason = reason;
  refresh();
}

void AgentIndicator::setAgents(const QList<ConnectedAgent>& agents) {
  for (const ConnectedAgent& agent : agents) {
    if (!m_previousClients.contains(agent.name)) m_previousClients << agent.name;
  }
  if (agents != m_agents) {
    m_agents = agents;
    refresh();
  }
}

void AgentIndicator::setChatWorking(const QString& agentName) {
  if (agentName != m_chatWorking) {
    m_chatWorking = agentName;
    refresh();
  }
}

AgentIndicator::State AgentIndicator::state() const {
  if (!m_agents.isEmpty() || !m_chatWorking.isEmpty()) {
    return State::Connected;
  }
  return m_unavailable ? State::Unavailable : State::NoAgent;
}

QString AgentIndicator::connectCommand() const {
  if (m_instance.isEmpty()) {
    return QString();
  }
  const QStringList arguments{QStringLiteral("mcp"), QStringLiteral("--connect"), QStringLiteral("--instance"),
                              m_instance};
  // The mounted bridge of an AppImage too: the command connects to this
  // window, which runs only while MitCad does (mitcad#1170).
  const QString bridge = bundledBridge();
  if (bridge.isEmpty()) {
    return QStringLiteral("mitcad-cli ") + arguments.join(QLatin1Char(' '));
  }
#ifdef Q_OS_WIN
  return assistantCommand(bridge, arguments, true);
#else
  return assistantCommand(bridge, arguments, false);
#endif
}

void AgentIndicator::openConnect() {
  qInfo().noquote() << QStringLiteral("Agent indicator chose help.connect_assistant");
  if (m_host.trigger) {
    m_host.trigger(QStringLiteral("help.connect_assistant"));
  }
}

QSize AgentIndicator::sizeHint() const {
  if (chromeStyle() != ChromeStyle::Floating) {
    return QToolButton::sizeHint();
  }
  return QSize(std::min(240, fontMetrics().horizontalAdvance(text()) + 12 + int(kDot) + 6 + kArrowWidth + 2), 30);
}

QSize AgentIndicator::minimumSizeHint() const {
  if (chromeStyle() != ChromeStyle::Floating) {
    return QToolButton::minimumSizeHint();
  }
  return QSize(12 + int(kDot) + 6 + 24 + kArrowWidth, 30);
}

void AgentIndicator::paintDot(QPainter& painter, const QRectF& box) const {
  const QRectF dot(box.center() - QPointF(kDot / 2, kDot / 2), QSizeF(kDot, kDot));
  QColor line = palette().color(QPalette::WindowText);
  switch (state()) {
  case State::Connected:
    painter.setPen(Qt::NoPen);
    painter.setBrush(accentColor(palette()));
    painter.drawEllipse(dot);
    break;
  case State::NoAgent:
    line.setAlpha(170);
    painter.setPen(QPen(line, 1.5));
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(dot.adjusted(0.75, 0.75, -0.75, -0.75));
    break;
  case State::Unavailable:
    line.setAlpha(120);
    painter.setPen(QPen(line, 1.5, Qt::SolidLine, Qt::RoundCap));
    painter.setBrush(Qt::NoBrush);
    painter.drawEllipse(dot.adjusted(0.75, 0.75, -0.75, -0.75));
    painter.drawLine(QPointF(dot.right() + 0.5, dot.top() - 0.5), QPointF(dot.left() - 0.5, dot.bottom() + 0.5));
    break;
  }
}

void AgentIndicator::paintEvent(QPaintEvent* event) {
  if (chromeStyle() != ChromeStyle::Floating) {
    QToolButton::paintEvent(event);
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  QColor fill = palette().color(QPalette::WindowText);
  fill.setAlpha(isDown() ? 35 : underMouse() ? 24 : 12);
  painter.setBrush(fill);
  painter.setPen(QPen(cardBorderColor(palette()), 1));
  painter.drawRoundedRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5), 10, 10);
  paintDot(painter, QRectF(12, 0, kDot, height()));
  painter.setPen(palette().color(QPalette::WindowText));
  const QRect label = rect().adjusted(12 + int(kDot) + 6, 0, -kArrowWidth, 0);
  painter.drawText(label, Qt::AlignVCenter | Qt::AlignLeft,
                   fontMetrics().elidedText(text(), Qt::ElideRight, label.width()));
  QColor ink = palette().color(QPalette::WindowText);
  if (popupMode() == QToolButton::MenuButtonPopup) {
    // The arrow is a part of its own: the button connects an agent.
    QColor divider = ink;
    divider.setAlpha(50);
    painter.setPen(QPen(divider, 1));
    const qreal x = width() - kArrowWidth + 3.5;
    painter.drawLine(QPointF(x, 7), QPointF(x, height() - 7));
  }
  painter.setPen(QPen(ink, 1.5, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
  painter.setBrush(Qt::NoBrush);
  const QPointF center(width() - 15, height() / 2.0);
  QPainterPath arrow;
  arrow.moveTo(center + QPointF(-3, -1.5));
  arrow.lineTo(center + QPointF(0, 1.5));
  arrow.lineTo(center + QPointF(3, -1.5));
  painter.drawPath(arrow);
}

bool AgentIndicator::arrowHit(const QPoint& point) const {
  return point.x() >= width() - kArrowWidth;
}

void AgentIndicator::mousePressEvent(QMouseEvent* event) {
  // The Floating capsule draws its own arrow, wider than the style's.
  if (chromeStyle() == ChromeStyle::Floating && popupMode() == QToolButton::MenuButtonPopup &&
      event->button() == Qt::LeftButton && arrowHit(event->position().toPoint())) {
    showMenu();
    return;
  }
  QToolButton::mousePressEvent(event);
}

void AgentIndicator::changeEvent(QEvent* event) {
  QToolButton::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::StyleChange) {
    refresh(); // the dot's colours
  }
}

void AgentIndicator::refresh() {
  const State now = state();
  QString shown;
  QString logged;
  QStringList tip;
  const QString place =
      m_instance.isEmpty() ? QString() : tr("Instance %1, port %2").arg(m_instance).arg(m_port);
  // Rows blocked in the menu (mitcad#1299): never hidden behind connected.
  const int blocked = int(blockedAssistantRows().size());
  // The log keeps the clients' own labels (UI tests match them).
  switch (now) {
  case State::NoAgent:
    shown = tr("No assistant");
    logged = QStringLiteral("no agent");
    tip << tr("No AI assistant connected; this window accepts them");
    if (!place.isEmpty()) {
      tip << place;
    }
    tip << tr("Click to connect an AI assistant");
    break;
  case State::Connected: {
    QStringList names;
    for (const ConnectedAgent& agent : std::as_const(m_agents)) {
      names << agent.name;
      tip << agentLine(agent);
    }
    if (!m_chatWorking.isEmpty()) {
      // The chat's agent at work on a message (mitcad#947).
      shown = tr("%1 working").arg(m_chatWorking);
      logged = QStringLiteral("chat working: %1").arg(m_chatWorking);
    } else if (m_agents.size() == 1) {
      // Beside a block, the count (mitcad#1299: "1 connected · 1 blocked").
      shown = blocked > 0 ? tr("%n connected", nullptr, 1)
                          : tr("%1 connected").arg(assistantDisplayName(m_agents.first().name));
      logged = QStringLiteral("connected: %1").arg(m_agents.first().name);
    } else {
      shown = blocked > 0 ? tr("%n connected", nullptr, int(m_agents.size()))
                          : tr("%n assistants connected", nullptr, int(m_agents.size()));
      logged = QStringLiteral("%1 agents connected: %2").arg(m_agents.size()).arg(names.join(QStringLiteral(", ")));
    }
    if (!place.isEmpty()) {
      tip << place;
    }
    tip << tr("Click for the assistants' menu");
    break;
  }
  case State::Unavailable:
    shown = tr("Assistants unavailable");
    logged = QStringLiteral("unavailable");
    tip << tr("AI assistants cannot connect to this window");
    if (!m_reason.isEmpty()) {
      tip << m_reason;
    }
    break;
  }
  if (blocked > 0 && now != State::Unavailable) {
    shown = tr("%1 · %n blocked", nullptr, blocked).arg(shown);
    logged = QStringLiteral("%1 · %2 blocked").arg(logged).arg(blocked);
    tip << tr("Blocked in this menu: %1").arg([] {
      QStringList names;
      for (const QString& key : blockedAssistantRows()) names << assistantRowName(key);
      return names.join(QStringLiteral(", "));
    }());
  }
  setPopupMode(now == State::NoAgent ? QToolButton::MenuButtonPopup : QToolButton::InstantPopup);
  setText(shown);
  setToolTip(tip.join(QLatin1Char('\n')));
  setAccessibleName(tr("AI assistant: %1").arg(shown));
  // The dot as the Docked status bar's icon; the Floating capsule paints it.
  const qreal ratio = std::max<qreal>(2.0, devicePixelRatioF());
  QPixmap pixmap(QSize(16, 16) * ratio);
  pixmap.setDevicePixelRatio(ratio);
  pixmap.fill(Qt::transparent);
  {
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    paintDot(painter, QRectF(0, 0, 16, 16));
  }
  setIcon(QIcon(pixmap));
  if (logged != m_logged) {
    m_logged = logged;
    qInfo().noquote() << QStringLiteral("Agent indicator: %1").arg(logged);
  }
  updateGeometry();
  update();
  logPlace();
  emit presentationChanged();
}

void AgentIndicator::moveEvent(QMoveEvent* event) {
  QToolButton::moveEvent(event);
  logPlace();
}

void AgentIndicator::resizeEvent(QResizeEvent* event) {
  QToolButton::resizeEvent(event);
  logPlace();
}

void AgentIndicator::logPlace() {
  if (m_placePending) {
    return;
  }
  m_placePending = true;
  // Where to click once the layout has placed it (UI tests): the button's
  // own part, left of the menu's arrow.
  TestSync::singleShot(50, this, [this] {
    m_placePending = false;
    if (!isVisible() || window() == nullptr) {
      return;
    }
    const QPoint center = mapTo(window(), rect().center());
    const QString place = QStringLiteral("%1,%2").arg(center.x()).arg(center.y());
    if (place != m_loggedPlace) {
      m_loggedPlace = place;
      qDebug().noquote() << QStringLiteral("Agent indicator at %1").arg(place);
    }
  });
}

QList<IncomingRow> AgentIndicator::incomingRows() const {
  const QStringList blocked = blockedAssistantRows();
  const QStringList remembered = rememberedAssistantRows();
  // The known assistants in their order, then the other clients as they
  // came: blocked first (kept), then this run's.
  QStringList keys;
  for (const QString& key : knownAssistantRows()) {
    bool shown = remembered.contains(key) || blocked.contains(key);
    for (const QString& label : m_previousClients) {
      shown = shown || assistantRowKey(label) == key;
    }
    for (const ConnectedAgent& agent : m_agents) {
      shown = shown || assistantRowKey(agent.name) == key;
    }
    if (shown) keys << key;
  }
  for (const QString& key : blocked) {
    if (!keys.contains(key) && assistantRowClient(key) == AssistantClient::Other) keys << key;
  }
  QStringList seen = m_previousClients;
  for (const ConnectedAgent& agent : m_agents) {
    if (!seen.contains(agent.name)) seen << agent.name;
  }
  for (const QString& label : seen) {
    const QString key = assistantRowKey(label);
    if (!keys.contains(key)) keys << key;
  }
  QList<IncomingRow> rows;
  for (const QString& key : keys) {
    IncomingRow row;
    row.key = key;
    row.name = assistantRowName(key);
    row.blocked = blocked.contains(key);
    QDateTime since;
    for (const ConnectedAgent& agent : m_agents) {
      if (assistantRowKey(agent.name) != key) continue;
      row.ids << agent.id;
      row.labels << tr("%1, connected since %2").arg(ownLabel(agent), shortTime(agent.since));
      if (!since.isValid() || agent.since < since) since = agent.since;
    }
    for (const QString& label : m_previousClients) {
      if (assistantRowKey(label) == key && !std::any_of(m_agents.cbegin(), m_agents.cend(), [&label](const ConnectedAgent& agent) {
            return agent.name == label;
          })) {
        row.labels << label;
      }
    }
    row.state = row.blocked        ? tr("blocked")
                : since.isValid() ? tr("connected since %1").arg(shortTime(since))
                                  : tr("not connected");
    rows << row;
  }
  return rows;
}

void AgentIndicator::setIncomingAllowed(const IncomingRow& row, bool allowed) {
  if (allowed) {
    allowAssistantRow(row.key);
    qInfo().noquote() << QStringLiteral("Agent indicator: %1 allowed").arg(row.name);
    return;
  }
  // Blocked first, so that its clients' reconnects are refused; then its
  // connections end.
  blockAssistantRow(row.key);
  qInfo().noquote() << QStringLiteral("Agent indicator: %1 blocked").arg(row.name);
  if (!row.ids.isEmpty() && m_host.disconnect) {
    m_host.disconnect(row.ids);
  }
}

void AgentIndicator::setOutgoingOffered(const OutgoingAgent& agent, bool offered) {
  qInfo().noquote() << QStringLiteral("Agent indicator: %1 %2").arg(agent.name, offered ? QStringLiteral("offered")
                                                                                       : QStringLiteral("off"));
  if (m_host.offer) {
    m_host.offer(agent.id, offered);
  }
}

void AgentIndicator::fillMenu() {
  m_menu->clear();
  const auto info = [this](const QString& text) {
    QAction* action = m_menu->addAction(text);
    action->setEnabled(false);
    action->setMenuRole(QAction::NoRole);
  };
  const auto add = [this](const QString& text, const QString& id, std::function<void()> run) {
    QAction* action = m_menu->addAction(text);
    action->setObjectName(id);
    action->setMenuRole(QAction::NoRole);
    connect(action, &QAction::triggered, this, [id, run = std::move(run)] {
      qInfo().noquote() << QStringLiteral("Agent indicator menu chose %1").arg(id);
      run();
    });
  };
  // A row of a section: its name, its state in the shortcut's column, a
  // checkmark; `chosen` gets the new check state.
  const auto row = [this](const QString& name, const QString& state, bool checked, const QString& id,
                          const QString& tip, std::function<void(bool)> chosen) {
    QString text = name;
    QAction* action = m_menu->addAction(text.replace(QLatin1Char('&'), QStringLiteral("&&")) + QLatin1Char('\t') + state);
    action->setObjectName(id);
    action->setProperty("agentRow", name);
    action->setMenuRole(QAction::NoRole);
    action->setCheckable(true);
    action->setChecked(checked);
    action->setToolTip(tip);
    connect(action, &QAction::triggered, this, [chosen = std::move(chosen)](bool now) { chosen(now); });
  };
  // Unavailable: why, first.
  if (m_unavailable) {
    info(m_reason.isEmpty() ? tr("AI assistants cannot connect to this window") : m_reason);
    m_menu->addSeparator();
  }
  // The assistants that drive MitCad (mitcad#1299): checked, they may
  // connect.
  info(tr("Incoming connections"));
  const QList<IncomingRow> incoming = incomingRows();
  for (const IncomingRow& entry : incoming) {
    QStringList tip = entry.labels;
    if (tip.isEmpty()) {
      tip << (assistantRowClient(entry.key) == AssistantClient::Other ? entry.name
                                                                     : tr("Set up with Connect AI Assistant"));
    }
    tip << (entry.blocked ? tr("Check to let it connect to MitCad again")
                          : tr("Uncheck to disconnect it and refuse its connections"));
    row(entry.name, entry.state, !entry.blocked, QStringLiteral("incomingAgent"), tip.join(QLatin1Char('\n')),
        [this, entry](bool allowed) { setIncomingAllowed(entry, allowed); });
  }
  if (incoming.isEmpty()) {
    info(tr("No assistant set up"));
  }
  m_menu->addSeparator();
  // The chat panel's agents: checked, the panel offers them.
  info(tr("Outgoing connections"));
  const QList<OutgoingAgent> outgoing = m_host.outgoing ? m_host.outgoing() : QList<OutgoingAgent>();
  for (const OutgoingAgent& agent : outgoing) {
    row(agent.name, agent.state, agent.offered, QStringLiteral("outgoingAgent"),
        agent.offered ? tr("Offered in the chat panel; uncheck to hide it there and never start it")
                      : tr("Check to offer it in the chat panel"),
        [this, agent](bool offered) { setOutgoingOffered(agent, offered); });
  }
  if (outgoing.isEmpty()) {
    info(tr("No chat agent found"));
  }
  m_menu->addSeparator();
  add(tr("&Connect AI Assistant…"), QStringLiteral("help.connect_assistant"), [this] {
    if (m_host.trigger) {
      m_host.trigger(QStringLiteral("help.connect_assistant"));
    }
  });
  // A command for this window only matters while another one listens too.
  const QString command = connectCommand();
  if (!command.isEmpty() && discover().size() > 1) {
    add(tr("Copy Co&mmand for This Window"), QStringLiteral("copyConnectCommand"), [command] {
      QApplication::clipboard()->setText(command);
      qInfo().noquote() << QStringLiteral("Agent indicator: copied %1").arg(command);
    });
  }
  add(tr("&Folders Assistants May Use…"), QStringLiteral("tools.preferences.agents"), [this] {
    if (m_host.trigger) {
      m_host.trigger(QStringLiteral("tools.preferences.agents"));
    }
  });
  QStringList entries;
  for (QAction* action : m_menu->actions()) {
    if (!action->isSeparator()) {
      entries << loggedEntry(action);
    }
  }
  qInfo().noquote() << QStringLiteral("Agent indicator menu: %1").arg(entries.join(QStringLiteral(" | ")));
}

} // namespace mitcad::mcp
