// SPDX-License-Identifier: MIT
#include "LiveServer.hpp"

#include <algorithm>
#include <cstddef>
#include <exception>
#include <stdexcept>
#include <vector>

#include <QApplication>
#include <QCoreApplication>
#include <QEvent>
#include <QDateTime>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QHash>
#include <QJsonObject>
#include <QPointer>
#include <QSettings>
#include <QStringList>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTimer>
#include <QUuid>

#include "MainWindow.hpp"
#include "WindowManager.hpp"
#include "framework/TitleBar.hpp"
#include "AgentIndicator.hpp"
#include "ChatGate.hpp"
#include "EndpointLimits.hpp"
#include "LiveErrors.hpp"
#include "LocalEndpoint.hpp"
#include "LogRing.hpp"
#include "LoopbackListener.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad::mcp {
namespace {

QString randomId() {
  return QUuid::createUuid().toString(QUuid::Id128);
}

QByteArray protocolError(int code, const QString& message) {
  return QJsonDocument(QJsonObject{{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
                                  {QStringLiteral("error"), QJsonObject{{QStringLiteral("code"), code},
                                                                      {QStringLiteral("message"), message}}}})
             .toJson(QJsonDocument::Compact) + '\n';
}

} // namespace

struct LiveServer::State {
  struct Client {
    QPointer<QTcpSocket> socket;
    QByteArray input;
    bool authenticated = false;
    // Initialized an MCP session: an agent is connected (mitcad#265).
    bool initialized = false;
    bool initializeAccepted = false;
    QString displayName;
    // clientInfo.version and when the session was initialized (mitcad#710).
    QString version;
    QDateTime since;
    // This connection among the others, to end it alone (mitcad#706).
    QString id = randomId();
    // When it came among the connections (mitcad#935): the oldest one that
    // has not authenticated gives way to a new one.
    quint64 order = 0;
    // The chat panel's marker of its own bridge connection (mitcad#947):
    // its calls go through the panel's approvals (ChatGate), and one that
    // waits for the user's answer holds the connection's next requests.
    QString chat;
    QPointer<MainWindow> chatWindow;
    QString document;
    bool waiting = false;
    rust::Box<LiveMcpSession> session = new_live_mcp_session();
    // The comments this session was last given (mitcad#1119).
    QByteArray commentsGiven;
    QString commentsDocument;
  };

  LiveServer* owner;
  QPointer<WindowManager> manager;
  QTcpServer listener;
  Endpoint endpoint;
  bool published = false;
  QString unavailable;
  bool handling = false;
  quint64 connections = 0;
  std::vector<std::shared_ptr<Client>> clients;
  struct Comments {
    QByteArray key;
    QByteArray text;
    QString error;
  };
  QHash<QString, Comments> comments;
  bool commentsPosted = false;

  State(LiveServer* object, WindowManager* windows) : owner(object), manager(windows) {}

  MainWindow* window() const { return manager ? manager->activeWindow() : nullptr; }

  MainWindow* documentWindow(const Client& client, const QJsonObject& arguments = {}) const {
    const QString id = arguments.value(QStringLiteral("document")).toString();
    if (!id.isEmpty()) return manager ? manager->windowForDocument(id) : nullptr;
    if (!client.chat.isEmpty())
      return client.chatWindow && client.chatWindow->mcpDocumentId() == client.document ? client.chatWindow.data() : nullptr;
    return window();
  }

  ChatGate* chatGate(const Client& client) const {
    return client.chatWindow ? client.chatWindow->chatGate() : nullptr;
  }

  void refreshComments() {
    bool pending = false;
    // Replaced and closed designs must not retain an unbounded comments cache.
    for (auto i = comments.begin(); i != comments.end();) {
      if (!manager || !manager->windowForDocument(i.key())) i = comments.erase(i);
      else ++i;
    }
    for (const auto& client : clients) {
      if (!client->authenticated || !connected(*client)) continue;
      MainWindow* target = !client->document.isEmpty() && manager
                               ? manager->windowForDocument(client->document) : documentWindow(*client);
      if (!target) {
        giveComments(*client, client->document, QJsonDocument(QJsonObject{{QStringLiteral("comments"), QJsonArray{}},
            {QStringLiteral("open"), 0}, {QStringLiteral("document"), client->document},
            {QStringLiteral("error"), QStringLiteral("The document has closed or been replaced")}}).toJson(QJsonDocument::Compact));
        continue;
      }
      if (target->modelBusy() || target->viewerWidget()->isPainting()) {
        // Never answer a newly selected design with another design's resource.
        if (client->commentsDocument != target->mcpDocumentId()) {
          giveComments(*client, target->mcpDocumentId(), QJsonDocument(QJsonObject{
              {QStringLiteral("document"), target->mcpDocumentId()}, {QStringLiteral("busy"), true},
              {QStringLiteral("retry_after_ms"), 200}}).toJson(QJsonDocument::Compact));
        }
        pending = true;
        continue;
      }
      auto& cached = comments[target->mcpDocumentId()];
      const QByteArray key = QJsonDocument(target->documentSession().snapshot().document).toJson(QJsonDocument::Compact);
      if (key != cached.key) {
        try {
          QJsonObject answer = target->query({{QStringLiteral("query"), QStringLiteral("comments")}}).toObject();
          answer.insert(QStringLiteral("document"), target->mcpDocumentId());
          const QByteArray text = QJsonDocument(answer).toJson(QJsonDocument::Compact);
          if (!cached.text.isEmpty() && cached.text != text)
            qInfo().noquote() << QStringLiteral("MCP: comments changed (%1 open)").arg(answer.value(QStringLiteral("open")).toInt());
          cached.text = text;
          cached.key = key;
          cached.error.clear();
        } catch (const std::exception& failure) {
          const QString error = QString::fromUtf8(failure.what());
          if (error != cached.error) qInfo().noquote() << QStringLiteral("MCP: the comments could not be read: %1").arg(error);
          cached.error = error;
        }
      }
      giveComments(*client, target->mcpDocumentId(), cached.text);
    }
    if (pending) scheduleComments(200);
  }

  void giveComments(Client& client, const QString& document, const QByteArray& text) {
    if (text.isEmpty() || client.commentsGiven == text) return;
    try {
      const std::string notification(client.session->set_mcp_comments(rust::Str(text.constData(), size_t(text.size()))));
      client.commentsGiven = text;
      client.commentsDocument = document;
      if (!notification.empty()) write(client, QByteArray::fromStdString(notification) + '\n');
    } catch (const std::exception& failure) {
      qWarning().noquote() << QStringLiteral("MCP: comments not given to a client: %1").arg(QString::fromUtf8(failure.what()));
    }
  }

  void scheduleComments(int delay) {
    if (commentsPosted || !manager || authenticatedClients() == 0) return;
    commentsPosted = true;
    QTimer::singleShot(delay, owner, [this] { commentsPosted = false; refreshComments(); });
  }

  void attachWindow(MainWindow* target) {
    target->installEventFilter(owner);
    QObject::connect(target, &QWidget::windowTitleChanged, owner, [this] { publishWindow(); });
    QObject::connect(&target->documentSession(), &DocumentSession::changed, owner, [this] { scheduleComments(0); });
    if (published) target->setMcpListener(endpoint.instance, endpoint.port);
    else if (!unavailable.isEmpty()) target->setMcpUnavailable(unavailable);
  }

  void failed(const QString& reason) {
    unavailable = QObject::tr("The listener for assistants did not start: %1").arg(reason);
    if (manager) for (MainWindow* target : manager->windows()) target->setMcpUnavailable(unavailable);
  }

  void publishWindow() {
    if (!published || !window()) return;
    // The design's name as the title bar shows it (mitcad#1149): not the
    // raw title, whose "[*]" is Qt's placeholder of the modified mark.
    endpoint.title = TitleBar::documentTitleOf(window()->windowTitle());
    QString error;
    if (!publishWindowState(endpoint, &error)) qWarning().noquote() << error;
  }

  void activated() {
    endpoint.activatedAt = QDateTime::currentMSecsSinceEpoch();
    publishWindow();
  }

  // The window shows whether agents are connected and can disconnect them
  // (mitcad#265), all or some (mitcad#706).
  void updateAgents() {
    QList<ConnectedAgent> agents;
    for (const auto& client : clients) {
      if (client->initialized && client->socket && client->socket->state() == QAbstractSocket::ConnectedState) {
        agents.append({client->displayName, client->version, client->since, client->id, !client->chat.isEmpty()});
      }
    }
    endpoint.agents.clear();
    for (const ConnectedAgent& agent : agents) {
      if (!endpoint.agents.contains(agent.name)) endpoint.agents.append(agent.name.left(256));
    }
    publishWindow();
    if (manager) for (MainWindow* target : manager->windows()) {
      QList<ConnectedAgent> shown;
      for (const auto& agent : agents) {
        const auto found = std::find_if(clients.begin(), clients.end(), [&](const auto& client) { return client->id == agent.id; });
        if (found != clients.end() && ((*found)->chat.isEmpty() || (*found)->chatWindow == target)) shown.append(agent);
      }
      target->setMcpAgents(shown, [this](const QStringList& ids) { disconnectClients(ids); });
    }
  }

  void disconnectClients(const QStringList& ids) {
    for (const auto& client : clients) {
      if (client->initialized && client->socket && ids.contains(client->id)) {
        client->socket->abort();
      }
    }
    updateAgents();
  }

  void write(Client& client, const QByteArray& response) {
    if (client.socket && client.socket->state() == QAbstractSocket::ConnectedState) {
      // The Rust dispatcher bounds each response to kMaxFrameBytes.
      // This separate queue budget protects against a client that stops reading.
      if (client.socket->bytesToWrite() + response.size() > 4 * (kMaxFrameBytes + 1)) {
        client.socket->abort();
      } else {
        client.socket->write(response);
      }
    }
  }

  static bool connected(const Client& client) {
    return client.socket && client.socket->state() == QAbstractSocket::ConnectedState;
  }

  int authenticatedClients() const {
    return static_cast<int>(std::count_if(clients.begin(), clients.end(), [](const auto& client) {
      return client->authenticated && connected(*client);
    }));
  }

  // A tool's result back to the client, through the session's pending
  // request.
  void finishTool(Client& client, const QJsonObject& value, bool error) {
    try {
      const QByteArray result = QJsonDocument(value).toJson(QJsonDocument::Compact);
      const std::string response(client.session->finish_mcp(rust::Str(result.constData(), size_t(result.size())), error));
      write(client, QByteArray::fromStdString(response) + '\n');
    } catch (const std::exception& failure) {
      write(client, protocolError(-32603, QString::fromUtf8(failure.what())));
    }
  }

  void runTool(Client& client, const QString& tool, const QJsonObject& arguments) {
    QJsonObject value;
    bool error = false;
    // The chat's own calls (mitcad#947): it tells its changes of the
    // design from the user's.
    const QString chat = client.chat;
    if (ChatGate* gate = chat.isEmpty() ? nullptr : chatGate(client)) {
      gate->toolStarted(chat, tool);
    }
    try {
      const QPointer<MainWindow> target(documentWindow(client, arguments));
      if (!target) {
        // A handle of a replaced or closed design names no window: the
        // client lists the open designs again, as with one window.
        if (!arguments.value(QStringLiteral("document")).toString().isEmpty())
          throw std::runtime_error(
              "No open document with this handle (unknown, closed or replaced); call document_list after New or Open");
        throw std::runtime_error("The application document window has closed");
      }
      // The live catalog (mitcad#265): MainWindowMcp.cpp, MainWindowMcpUi.cpp.
      // An applied model command is an error only when a feature it
      // added or edited fails; finish_mcp decides (mitcad#401).
      if (!client.chat.isEmpty() && target != client.chatWindow)
        throw std::runtime_error("A chat agent can only access its own document");
      const QString document = target->mcpDocumentId();
      value = target->mcpTool(tool, arguments);
      if (!target) {
        client.document = document;
        throw std::runtime_error("The application document window closed during the request");
      }
      client.document = target->mcpDocumentId();
      scheduleComments(0);
    } catch (const Refusal& refusal) {
      // Busy (when to retry), a panel's state.
      error = true;
      value = refusal.fields();
      value.insert(QStringLiteral("error"), QString::fromUtf8(refusal.what()));
    } catch (const std::exception& failure) {
      error = true;
      value = {{QStringLiteral("error"), QString::fromUtf8(failure.what())}};
    }
    if (ChatGate* gate = chat.isEmpty() ? nullptr : chatGate(client)) {
      gate->toolFinished(chat, tool, error);
    }
    finishTool(client, value, error);
  }

  // The user answered a chat call's card: run it or refuse it, then the
  // connection's next requests.
  void resume(const std::weak_ptr<Client>& weak, const QString& tool, const QJsonObject& arguments, bool allowed,
              const QString& reason) {
    const std::shared_ptr<Client> client = weak.lock();
    if (!client || !client->waiting || !connected(*client)) {
      return;
    }
    if (handling) {
      // Another client's model command runs Qt's progress loop: after it.
      QTimer::singleShot(20, owner, [this, weak, tool, arguments, allowed, reason] {
        resume(weak, tool, arguments, allowed, reason);
      });
      return;
    }
    client->waiting = false;
    handling = true;
    if (allowed) {
      runTool(*client, tool, arguments);
    } else {
      finishTool(*client,
                 {{QStringLiteral("error"), reason.isEmpty() ? QObject::tr("The user did not allow this.") : reason},
                  {QStringLiteral("denied"), true}},
                 true);
    }
    handling = false;
    drain(client);
  }

  void frame(const std::shared_ptr<Client>& shared, const QByteArray& message) {
    Client& client = *shared;
    if (!client.authenticated) {
      const auto object = QJsonDocument::fromJson(message).object();
      // The chat panel's own connection carries its marker beside the token
      // (mitcad#947).
      const QString marker = object.value(QStringLiteral("mitcadChat")).toString();
      const bool chatMarked = object.size() == 2 && !marker.isEmpty() && marker.size() <= kMaxChatMarker;
      // The token in constant time; only authenticated connections hold
      // the clients' slots (mitcad#935).
      bool accepted = (object.size() == 1 || chatMarked) &&
                      sameToken(object.value(QStringLiteral("mitcadAuth")).toString(), endpoint.token);
      const bool full = accepted && !clientSlotFree(authenticatedClients());
      accepted = accepted && !full;
      write(client, accepted ? QByteArray("{\"authenticated\":true}\n")
                    : full   ? QByteArray("{\"authenticated\":false,\"error\":\"too many clients\"}\n")
                             : QByteArray("{\"authenticated\":false}\n"));
      client.authenticated = accepted;
      if (accepted && chatMarked) {
        client.chat = marker;
        if (manager) for (MainWindow* target : manager->windows()) {
          if (target->chatGate() && target->chatGate()->ownsMarker(marker)) { client.chatWindow = target; client.document = target->mcpDocumentId(); break; }
        }
      }
      if (!accepted && client.socket) {
        client.socket->disconnectFromHost();
      }
      return;
    }
    if (QString::fromUtf8(message).toUtf8() != message) {
      write(client, protocolError(-32700, QStringLiteral("Invalid UTF-8 JSON message")));
      return;
    }
    // Comments for agents (mitcad#1119): whether agents may resolve them
    // (Preferences › AI Assistants), and the design's comments as they are
    // now, before the request reads them.
    client.session->set_mcp_comment_policy(
        QSettings().value(QStringLiteral("ai/agentsMayResolveComments"), false).toBool());
    refreshComments();
    try {
      const std::string planText(client.session->prepare_mcp(rust::Str(message.constData(), size_t(message.size()))));
      const auto plan = QJsonDocument::fromJson(QByteArray::fromStdString(planText)).object();
      if (plan.contains(QStringLiteral("response"))) {
        const auto response = plan.value(QStringLiteral("response"));
        const auto request = QJsonDocument::fromJson(message).object();
        if (!response.isNull()) {
          // The Rust dispatcher serializes IDs losslessly, including u64.
          write(client, response.toString().toUtf8() + '\n');
          if (!client.initializeAccepted &&
              request.value(QStringLiteral("method")).toString() ==
                  QStringLiteral("initialize") &&
              QJsonDocument::fromJson(response.toString().toUtf8()).object().contains(QStringLiteral("result"))) {
            client.initializeAccepted = true;
            const QJsonObject info = request.value(QStringLiteral("params")).toObject()
                .value(QStringLiteral("clientInfo")).toObject();
            const auto label = [](QString text, qsizetype length) {
              for (QChar& character : text) if (!character.isPrint()) character = QLatin1Char(' ');
              return text.simplified().left(length);
            };
            client.displayName = label(info.value(QStringLiteral("name")).toString(), 80);
            client.version = label(info.value(QStringLiteral("version")).toString(), 40);
            if (client.displayName.isEmpty()) client.displayName = QObject::tr("Unnamed client");
          }
        } else if (!client.initialized && client.initializeAccepted &&
                   request.value(QStringLiteral("jsonrpc")).toString() == QStringLiteral("2.0") &&
                   !request.contains(QStringLiteral("id")) &&
                   (!request.contains(QStringLiteral("params")) || request.value(QStringLiteral("params")).isObject()) &&
                   request.value(QStringLiteral("method")).toString() == QStringLiteral("notifications/initialized")) {
          client.initialized = true;
          client.since = QDateTime::currentDateTime();
          updateAgents();
        }
        return;
      }
      const QString tool = plan.value(QStringLiteral("tool")).toString();
      auto arguments = plan.value(QStringLiteral("arguments")).toObject();
      // Bind an omitted handle before a user approval can change the active window.
      if (!arguments.contains(QStringLiteral("document"))) {
        if (MainWindow* target = documentWindow(client)) arguments.insert(QStringLiteral("document"), target->mcpDocumentId());
      }
      if (!client.chat.isEmpty()) {
        // The chat's own connection (mitcad#947): its approvals first.
        ChatGate* gate = chatGate(client);
        if (gate == nullptr) {
          finishTool(client, {{QStringLiteral("error"), QObject::tr("The chat that started this agent has closed.")}},
                     true);
          return;
        }
        const std::weak_ptr<Client> weak = shared;
        const QPointer<QObject> server(owner);
        const ChatGate::Decision decision = gate->decide(
            client.chat, tool, arguments, [this, server, weak, tool, arguments](bool allowed, const QString& reason) {
              if (server) {
                QTimer::singleShot(0, server.data(), [this, weak, tool, arguments, allowed, reason] {
                  resume(weak, tool, arguments, allowed, reason);
                });
              }
            });
        if (decision.verdict == ChatGate::Verdict::Deny) {
          finishTool(client, {{QStringLiteral("error"), decision.reason}, {QStringLiteral("denied"), true}}, true);
          return;
        }
        if (decision.verdict == ChatGate::Verdict::Ask) {
          client.waiting = true;
          return;
        }
      }
      runTool(client, tool, arguments);
    } catch (const std::exception& failure) {
      write(client, protocolError(-32603, QString::fromUtf8(failure.what())));
    }
  }

  void drain(const std::shared_ptr<Client>& client) {
    if (!client->socket || client->socket->state() != QAbstractSocket::ConnectedState ||
        (handling && client->authenticated) || client->waiting) {
      return;
    }
    const bool alreadyHandling = handling;
    handling = true;
    int processed = 0;
    while (processed < 32 && client->socket && client->socket->state() == QAbstractSocket::ConnectedState) {
      // Discovery can authenticate a new connection while another model
      // command's progress loop runs. Its tools wait until that call ends.
      if (alreadyHandling && client->authenticated) {
        break;
      }
      const qsizetype limit = client->authenticated ? kMaxFrameBytes : 4096;
      const qsizetype newline = client->input.indexOf('\n');
      if (newline >= 0) {
        if (newline > limit) {
          write(*client, protocolError(-32600, QStringLiteral("Message exceeds the input limit")));
          client->socket->disconnectFromHost();
          break;
        }
        QByteArray message = client->input.left(newline);
        client->input.remove(0, newline + 1);
        frame(client, message);
        ++processed;
        if (client->waiting) {
          break; // the user's answer comes first (mitcad#947)
        }
        continue;
      }
      if (client->input.size() > limit) {
        write(*client, protocolError(-32600, QStringLiteral("Message exceeds the input limit")));
        client->socket->disconnectFromHost();
        break;
      }
      if (!client->socket->bytesAvailable()) {
        break;
      }
      client->input += client->socket->read(limit + 1 - client->input.size());
    }
    handling = alreadyHandling;
    if (alreadyHandling) {
      return;
    }
    // Model commands run Qt's progress loop. Other clients' readyRead
    // callbacks can arrive inside it; consume them only once it finishes.
    QTimer::singleShot(0, owner, [this] {
      const auto waiting = clients;
      for (const auto& other : waiting) {
        if (other->socket && (other->socket->bytesAvailable() || other->input.contains('\n') ||
                              other->input.size() > (other->authenticated ? kMaxFrameBytes : 4096))) {
          drain(other);
        }
      }
    });
  }

  void accept() {
    while (listener.hasPendingConnections()) {
      auto* socket = listener.nextPendingConnection();
      clients.erase(std::remove_if(clients.begin(), clients.end(), [](const auto& client) { return !client->socket; }),
                    clients.end());
      // Connections that have not authenticated hold no client slot; when
      // kMaxPending of them wait, the oldest gives way (mitcad#935): idle
      // connections of another local account cannot keep agents out. The
      // lines already written are read first: a burst of new connections
      // closes only connections that have not authenticated by then, not an
      // agent's whose line was still unread.
      const std::vector<std::shared_ptr<Client>> known = clients;
      for (const auto& waiting : known) {
        if (!waiting->authenticated && connected(*waiting) &&
            (waiting->socket->bytesAvailable() > 0 || waiting->socket->waitForReadyRead(0))) {
          drain(waiting);
        }
      }
      std::vector<std::shared_ptr<Client>> pending;
      std::vector<quint64> since;
      for (const auto& client : clients) {
        if (!client->authenticated && connected(*client)) {
          pending.push_back(client);
          since.push_back(client->order);
        }
      }
      const int oldest = pendingToClose(since);
      if (oldest >= 0) {
        pending[static_cast<std::size_t>(oldest)]->socket->abort();
      }
      socket->setParent(owner);
      socket->setReadBufferSize(kMaxFrameBytes + 1);
      auto client = std::make_shared<Client>();
      client->socket = socket;
      client->order = ++connections;
      clients.push_back(client);
      QObject::connect(socket, &QTcpSocket::readyRead, owner, [this, client] { drain(client); });
      const std::weak_ptr<Client> weak = client;
      QObject::connect(socket, &QTcpSocket::disconnected, owner, [this, socket, weak] {
        socket->deleteLater();
        // The chat's agent ended (mitcad#947): its card goes.
        if (const std::shared_ptr<Client> gone = weak.lock(); gone && !gone->chat.isEmpty()) {
          gone->waiting = false;
          if (ChatGate* gate = chatGate(*gone)) {
            gate->connectionClosed(gone->chat);
          }
        }
        // Once the socket's QPointer has cleared.
        QTimer::singleShot(0, owner, [this] { updateAgents(); });
      });
      QTimer::singleShot(5000, socket, [client] {
        if (!client->authenticated && client->socket) {
          client->socket->abort();
        }
      });
    }
  }
};

LiveServer::LiveServer(WindowManager* manager, QObject* parent)
    : QObject(parent), m_state(std::make_unique<State>(this, manager)) {
  for (MainWindow* window : manager->windows()) m_state->attachWindow(window);
  connect(manager, &WindowManager::windowAdded, this, [this](MainWindow* window) {
    m_state->attachWindow(window);
    m_state->updateAgents();
  });
  connect(manager, &WindowManager::windowsChanged, this, [this] { m_state->scheduleComments(0); m_state->updateAgents(); });
  connect(manager, &WindowManager::activeWindowChanged, this, [this] {
    // Unbound external clients follow focus; chat clients remain document-bound.
    for (const auto& client : m_state->clients) if (client->chat.isEmpty()) client->document.clear();
    m_state->activated();
    m_state->scheduleComments(0);
  });
  connect(qApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state == Qt::ApplicationActive) m_state->activated();
  });
}

bool LiveServer::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_state->window() && event->type() == QEvent::WindowActivate) m_state->activated();
  return QObject::eventFilter(watched, event);
}

LiveServer::~LiveServer() {
  m_state->listener.close();
  if (m_state->manager) for (MainWindow* window : m_state->manager->windows()) window->setMcpAgents({}, {});
  for (const auto& client : m_state->clients) {
    if (client->socket) { client->socket->disconnect(this); client->socket->abort(); }
    if (ChatGate* gate = m_state->chatGate(*client); gate && !client->chat.isEmpty()) gate->connectionClosed(client->chat);
  }
  if (m_state->published) removeEndpoint(m_state->endpoint);
}

bool LiveServer::start(QString* error) {
  // ui_log's lines (mitcad#265): main() installs the ring for the whole run
  // (mcp/LogRing.hpp); this keeps it for a server started without it.
  installLogRing();
  if (!prepareDiscoveryDirectory(error)) {
    m_state->failed(error ? *error : tr("Unknown listener error"));
    return false;
  }
  // Removes the records of killed instances (mitcad#437) before a client
  // has to skip them.
  discover();
  // On Windows with SO_EXCLUSIVEADDRUSE (mitcad#945).
  if (!listenOnLoopback(m_state->listener, error)) {
    m_state->failed(error ? *error : tr("Unknown listener error"));
    return false;
  }
  auto& endpoint = m_state->endpoint;
  endpoint.instance = randomId();
  endpoint.token = randomId() + randomId();
  endpoint.pid = QCoreApplication::applicationPid();
  endpoint.port = m_state->listener.serverPort();
  if (!publishEndpoint(endpoint, error)) {
    m_state->listener.close();
    m_state->failed(error ? *error : tr("Unknown listener error"));
    return false;
  }
  m_state->published = true;
  m_state->unavailable.clear();
  if (m_state->window() && QGuiApplication::applicationState() == Qt::ApplicationActive) m_state->activated();
  else m_state->publishWindow();
  connect(&m_state->listener, &QTcpServer::newConnection, this, [this] { m_state->accept(); });
  qInfo().noquote() << "MCP live instance:" << endpoint.instance << "127.0.0.1:" << endpoint.port;
  // The agent indicator names them (mitcad#710).
  if (m_state->manager) for (MainWindow* window : m_state->manager->windows())
    window->setMcpListener(endpoint.instance, endpoint.port);
  return true;
}

} // namespace mitcad::mcp
