// SPDX-License-Identifier: MIT
#pragma once

// The agent indicator (mitcad#710): whether AI assistants are connected to
// this window through live MCP (mitcad#265), beside the project indicator:
// in the title row of the Floating layout, the status bar of the Docked one
// and the toolbar of the native macOS window. Always shown, in one of three
// states, told apart by its text and its dot:
//
//   No assistant             (a hollow dot)  the window accepts assistants
//   Claude Code connected    (an accent dot) one; "2 assistants connected"
//   Assistants unavailable   (a struck dot)  the listener did not start
//
// A connected client is named by the assistant its label matches
// (assistantDisplayName), else by its own label. Rows blocked in its menu
// follow: "1 connected · 1 blocked" (mitcad#1299). With none connected a
// click opens Connect AI Assistant (the one place to set up and start a
// connection, mitcad#682), and the arrow (or a right click) the menu; in
// the other states a click opens the menu: the reason assistants are
// unavailable, then two sections (mitcad#1299). Incoming connections: a
// checkable row per assistant that drives MitCad (set up, blocked, or
// connected in this run; assistantRowKey) with its state, connected since,
// not connected or blocked; unchecking blocks it (and ends its
// connections), checking allows it again. Outgoing connections: a checkable
// row per agent the chat panel found, with the panel's state; unchecked, the
// panel neither lists nor starts it. Then Connect AI Assistant..., Copy
// Command for This Window (only while several MitCad windows listen) and
// Folders Assistants May Use... (Preferences > AI Assistants). The clients'
// own labels and versions are in the rows' tooltips; the listener's
// instance id and port in the indicator's.

#include <functional>

#include <QDateTime>
#include <QList>
#include <QString>
#include <QStringList>
#include <QToolButton>

class QMenu;
class QPainter;

namespace mitcad::mcp {

// An initialized MCP session: what its client said of itself in
// `initialize` (clientInfo: labels, not identities), since when, and the
// listener's id of the connection (to end this one only).
struct ConnectedAgent {
  QString name;
  QString version;
  QDateTime since;
  QString id;
  // The chat panel's own agent (mitcad#947).
  bool chat = false;

  bool operator==(const ConnectedAgent& other) const {
    return name == other.name && version == other.version && since == other.since && id == other.id &&
           chat == other.chat;
  }
  bool operator!=(const ConnectedAgent& other) const { return !(*this == other); }
};

// An agent of the chat panel (mitcad#1299): its id, its name in the menu,
// its state as the panel shows it, and whether the panel offers it.
struct OutgoingAgent {
  QString id;
  QString name;
  QString state;
  bool offered = true;
};

// A row of Incoming connections (mitcad#1299): its key (assistantRowKey),
// name and state, the labels and versions its clients gave (the tooltip),
// whether it is blocked, and its connected clients' ids.
struct IncomingRow {
  QString key;
  QString name;
  QString state;
  QStringList labels;
  bool blocked = false;
  QStringList ids;
};

class AgentIndicator : public QToolButton {
  Q_OBJECT

public:
  enum class State { NoAgent, Connected, Unavailable };

  struct Host {
    // Runs a command of the registry by its id (help.connect_assistant,
    // tools.preferences.agents).
    std::function<void(const QString& id)> trigger;
    // Ends these clients' connections (ConnectedAgent::id): the listener's.
    std::function<void(const QStringList& ids)> disconnect;
    // The chat panel's agents, and turning one on or off (mitcad#1299).
    std::function<QList<OutgoingAgent>()> outgoing;
    std::function<void(const QString& id, bool offered)> offer;
  };

  explicit AgentIndicator(Host host, QWidget* parent = nullptr);

  // The listener started: its instance id and port.
  void setListener(const QString& instance, int port);
  // The listener did not start (or was not asked to), and why.
  void setUnavailable(const QString& reason);
  void setAgents(const QList<ConnectedAgent>& agents);
  // The chat panel's agent while it works on a message (mitcad#947): "Claude
  // Code working" instead of the connections; empty when it does not.
  void setChatWorking(const QString& agentName);

  State state() const;
  const QList<ConnectedAgent>& agents() const { return m_agents; }
  // Clients initialized in this run, also while the setup dialog was closed.
  const QStringList& previousClients() const { return m_previousClients; }
  // The rows of Incoming connections, in the menu's order (mitcad#1299).
  QList<IncomingRow> incomingRows() const;
  // A row unchecked (blocked: its clients end and are refused) or checked
  // (allowed again), as its menu entry does; logged.
  void setIncomingAllowed(const IncomingRow& row, bool allowed);
  // An outgoing agent turned on or off, as its menu entry does; logged.
  void setOutgoingOffered(const OutgoingAgent& agent, bool offered);
  // The listener's instance id (empty while there is none) and why there is
  // none (the chat panel, mitcad#947).
  const QString& listenerInstance() const { return m_instance; }
  const QString& unavailableReason() const { return m_reason; }
  // The bridge's command for this window, "" while there is no listener.
  QString connectCommand() const;
  // Its menu, filled when it is about to show (native chrome shows it too).
  QMenu* agentMenu() const { return m_menu; }
  // A click with no assistant connected (and Connect AI Assistant...):
  // Connect AI Assistant.
  void openConnect();

  // The text shown, and the one logged with the clients' own labels
  // ("connected: claude-code").
  QString shownText() const { return text(); }
  const QString& loggedText() const { return m_logged; }
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

signals:
  // Native chrome observes the same text, tooltip and state.
  void presentationChanged();

protected:
  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void changeEvent(QEvent* event) override;

private:
  void refresh();
  void fillMenu();
  void logPlace();
  // The dot of the state within `box`, in the palette's colours.
  void paintDot(QPainter& painter, const QRectF& box) const;
  bool arrowHit(const QPoint& point) const;

  Host m_host;
  QString m_instance;
  int m_port = 0;
  bool m_unavailable = false;
  QString m_reason;
  QList<ConnectedAgent> m_agents;
  QStringList m_previousClients;
  QString m_chatWorking;
  QMenu* m_menu = nullptr;
  QString m_logged;
  QString m_loggedPlace;
  bool m_placePending = false;
};

} // namespace mitcad::mcp
