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

// "Claude Code (claude-code 2.1.0), connected since 14:02": the assistant's
// name, then what the client called itself where that differs.
QString agentLine(const ConnectedAgent& agent) {
  const QString shown = assistantDisplayName(agent.name);
  const QString own = agent.version.isEmpty() ? agent.name : QString(agent.name + QLatin1Char(' ') + agent.version);
  // The chat panel's own agent (mitcad#947).
  const QString chat = agent.chat ? AgentIndicator::tr(", the chat's") : QString();
  return shown == agent.name
             ? AgentIndicator::tr("%1%2, connected since %3").arg(own, chat, shortTime(agent.since))
             : AgentIndicator::tr("%1 (%2)%3, connected since %4").arg(shown, own, chat, shortTime(agent.since));
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
  setMenu(m_menu);
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
  const QString bridge = installedBridge(QCoreApplication::applicationDirPath());
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
      shown = tr("%1 connected").arg(assistantDisplayName(m_agents.first().name));
      logged = QStringLiteral("connected: %1").arg(m_agents.first().name);
    } else {
      shown = tr("%n assistants connected", nullptr, int(m_agents.size()));
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

void AgentIndicator::fillMenu() {
  m_menu->clear();
  bool details = false;
  const auto info = [this, &details](const QString& text) {
    QAction* action = m_menu->addAction(text);
    action->setEnabled(false);
    action->setMenuRole(QAction::NoRole);
    details = true;
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
  // Unavailable: why, first.
  if (m_unavailable) {
    info(m_reason.isEmpty() ? tr("AI assistants cannot connect to this window") : m_reason);
  }
  for (const ConnectedAgent& agent : std::as_const(m_agents)) {
    info(agentLine(agent));
  }
  if (details) {
    m_menu->addSeparator();
  }
  if (!m_agents.isEmpty()) {
    add(m_agents.size() == 1 ? tr("&Disconnect %1").arg(assistantDisplayName(m_agents.first().name))
                             : tr("&Disconnect All"),
        QStringLiteral("disconnectAgent"), [this] {
          qInfo().noquote() << QStringLiteral("Agent disconnected by the user");
          if (m_host.disconnectAll) {
            m_host.disconnectAll();
          }
        });
  }
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
  m_menu->addSeparator();
  add(tr("&Folders for Designs…"), QStringLiteral("tools.preferences.agents"), [this] {
    if (m_host.trigger) {
      m_host.trigger(QStringLiteral("tools.preferences.agents"));
    }
  });
  QStringList entries;
  for (QAction* action : m_menu->actions()) {
    if (!action->isSeparator()) {
      entries << action->text().remove(QLatin1Char('&'));
    }
  }
  qInfo().noquote() << QStringLiteral("Agent indicator menu: %1").arg(entries.join(QStringLiteral(" | ")));
}

} // namespace mitcad::mcp
