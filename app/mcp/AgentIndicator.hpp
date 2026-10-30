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
// (assistantDisplayName), else by its own label. With none connected a
// click opens Connect AI Assistant (the one place to set up and start a
// connection, mitcad#682), and the arrow (or a right click) the menu; in
// the other states a click opens the menu: the reason assistants are
// unavailable, or each connected client (name, its own label and version,
// connected since), then Disconnect <name> (Disconnect All), Connect AI
// Assistant..., Copy Command for This Window (only while several Mitcad
// windows listen) and Folders for Designs... (Preferences > AI Assistants). The listener's
// instance id and port are in the tooltip.

#include <functional>

#include <QDateTime>
#include <QList>
#include <QString>
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

class AgentIndicator : public QToolButton {
  Q_OBJECT

public:
  enum class State { NoAgent, Connected, Unavailable };

  struct Host {
    // Runs a command of the registry by its id (help.connect_assistant,
    // tools.preferences.agents).
    std::function<void(const QString& id)> trigger;
    // Ends every agent's connection (the listener's).
    std::function<void()> disconnectAll;
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
  QString m_chatWorking;
  QMenu* m_menu = nullptr;
  QString m_logged;
  QString m_loggedPlace;
  bool m_placePending = false;
};

} // namespace mitcad::mcp
