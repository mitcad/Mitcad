// SPDX-License-Identifier: MIT
// An API agent's key across a change in Preferences (mitcad#1309): the
// Anthropic API agent against a local service that records each request's
// x-api-key, and a stand-in of MitCad's listener for the chat's own
// connection (its tools, a call the test answers). Preferences' signal
// (ChatNotifier::apiSettingsChanged) between two requests of one message:
// the message's next request and the next message's first one carry the
// key; once the key is removed, the next message ends the conversation
// with a notice instead of a request without the key. Keys are kept for
// the run only (MITCAD_TEST_KEYCHAIN=off). An executable of its own
// (app.api_agent): it needs the Rust bridge and Qt Network.
#include "agent/ApiAgent.hpp"
#include "agent/ChatSettings.hpp"
#include "mcp/LocalEndpoint.hpp"

#include <cstdio>
#include <functional>

#include <QByteArray>
#include <QCoreApplication>
#include <QEventLoop>
#include <QHash>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QNetworkProxy>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QTimer>

using mitcad::agent::ApiAgent;
using mitcad::agent::ChatAgent;
using mitcad::agent::ChatNotifier;

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "api_agent.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kKey = QStringLiteral("sk-test-1309-key");

// Runs the event loop until `done` holds, at most `ms`.
bool waitFor(const std::function<bool()>& done, int ms = 20000) {
  if (done()) {
    return true;
  }
  QEventLoop loop;
  QTimer poll;
  poll.setInterval(10);
  QObject::connect(&poll, &QTimer::timeout, &loop, [&loop, &done] {
    if (done()) {
      loop.quit();
    }
  });
  QTimer limit;
  limit.setSingleShot(true);
  QObject::connect(&limit, &QTimer::timeout, &loop, &QEventLoop::quit);
  poll.start();
  limit.start(ms);
  loop.exec();
  return done();
}

QByteArray line(const QJsonObject& message) { return QJsonDocument(message).toJson(QJsonDocument::Compact) + '\n'; }

// MitCad's listener as the chat's connection sees it: it takes the
// authentication line, answers initialize and tools/list, and keeps each
// tools/call for the test to answer.
class Listener : public QObject {
public:
  Listener() {
    m_server.listen(QHostAddress(QHostAddress::LocalHost));
    connect(&m_server, &QTcpServer::newConnection, this, [this] {
      m_socket = m_server.nextPendingConnection();
      connect(m_socket, &QTcpSocket::readyRead, this, [this] { read(); });
    });
  }

  quint16 port() const { return m_server.serverPort(); }
  bool authenticated() const { return m_authenticated; }
  const QList<QJsonObject>& calls() const { return m_calls; }

  void answer(const QJsonObject& call) {
    const QJsonObject text{{QStringLiteral("type"), QStringLiteral("text")}, {QStringLiteral("text"), QStringLiteral("{}")}};
    write({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
           {QStringLiteral("id"), call.value(QStringLiteral("id"))},
           {QStringLiteral("result"), QJsonObject{{QStringLiteral("content"), QJsonArray{text}}}}});
  }

private:
  void write(const QJsonObject& message) {
    if (m_socket != nullptr) {
      m_socket->write(line(message));
    }
  }

  void read() {
    m_input += m_socket->readAll();
    for (qsizetype newline = m_input.indexOf('\n'); newline >= 0; newline = m_input.indexOf('\n')) {
      const QJsonObject message = QJsonDocument::fromJson(m_input.left(newline)).object();
      m_input.remove(0, newline + 1);
      if (!m_authenticated) {
        m_authenticated = message.contains(QStringLiteral("mitcadAuth"));
        write({{QStringLiteral("authenticated"), m_authenticated}});
        continue;
      }
      const QString method = message.value(QStringLiteral("method")).toString();
      const QJsonValue id = message.value(QStringLiteral("id"));
      if (method == QStringLiteral("initialize")) {
        write({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
               {QStringLiteral("id"), id},
               {QStringLiteral("result"), QJsonObject{{QStringLiteral("protocolVersion"), QStringLiteral("2025-06-18")}}}});
      } else if (method == QStringLiteral("tools/list")) {
        const QJsonObject tool{{QStringLiteral("name"), QStringLiteral("model_query")},
                               {QStringLiteral("description"), QStringLiteral("Queries the model.")},
                               {QStringLiteral("inputSchema"), QJsonObject{{QStringLiteral("type"), QStringLiteral("object")}}}};
        write({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
               {QStringLiteral("id"), id},
               {QStringLiteral("result"), QJsonObject{{QStringLiteral("tools"), QJsonArray{tool}}}}});
      } else if (method == QStringLiteral("tools/call")) {
        m_calls.append(message);
      }
    }
  }

  QTcpServer m_server;
  QTcpSocket* m_socket = nullptr;
  QByteArray m_input;
  bool m_authenticated = false;
  QList<QJsonObject> m_calls;
};

// One server-sent event of the Messages API.
QByteArray event(const char* name, const QJsonObject& data) {
  return QByteArrayLiteral("event: ") + name + QByteArrayLiteral("\ndata: ") +
         QJsonDocument(data).toJson(QJsonDocument::Compact) + QByteArrayLiteral("\n\n");
}

QByteArray messageEnd(const char* reason) {
  return event("message_delta",
               {{QStringLiteral("type"), QStringLiteral("message_delta")},
                {QStringLiteral("delta"), QJsonObject{{QStringLiteral("stop_reason"), QString::fromLatin1(reason)}}}}) +
         event("message_stop", {{QStringLiteral("type"), QStringLiteral("message_stop")}});
}

// An answer that calls model_query.
QByteArray callAnswer() {
  const QJsonObject block{{QStringLiteral("type"), QStringLiteral("tool_use")},
                          {QStringLiteral("id"), QStringLiteral("toolu_1309")},
                          {QStringLiteral("name"), QStringLiteral("model_query")},
                          {QStringLiteral("input"), QJsonObject{}}};
  const QJsonObject delta{{QStringLiteral("type"), QStringLiteral("input_json_delta")},
                          {QStringLiteral("partial_json"), QStringLiteral("{\"query\": \"bodies\"}")}};
  return event("content_block_start", {{QStringLiteral("type"), QStringLiteral("content_block_start")},
                                       {QStringLiteral("index"), 0},
                                       {QStringLiteral("content_block"), block}}) +
         event("content_block_delta", {{QStringLiteral("type"), QStringLiteral("content_block_delta")},
                                       {QStringLiteral("index"), 0},
                                       {QStringLiteral("delta"), delta}}) +
         event("content_block_stop", {{QStringLiteral("type"), QStringLiteral("content_block_stop")},
                                      {QStringLiteral("index"), 0}}) +
         messageEnd("tool_use");
}

// An answer of text only.
QByteArray textAnswer() {
  const QJsonObject block{{QStringLiteral("type"), QStringLiteral("text")}, {QStringLiteral("text"), QString()}};
  const QJsonObject delta{{QStringLiteral("type"), QStringLiteral("text_delta")}, {QStringLiteral("text"), QStringLiteral("Done.")}};
  return event("content_block_start", {{QStringLiteral("type"), QStringLiteral("content_block_start")},
                                       {QStringLiteral("index"), 0},
                                       {QStringLiteral("content_block"), block}}) +
         event("content_block_delta", {{QStringLiteral("type"), QStringLiteral("content_block_delta")},
                                       {QStringLiteral("index"), 0},
                                       {QStringLiteral("delta"), delta}}) +
         event("content_block_stop", {{QStringLiteral("type"), QStringLiteral("content_block_stop")},
                                      {QStringLiteral("index"), 0}}) +
         messageEnd("end_turn");
}

// The service: GET lists one model, each POST (a message's request) is
// answered by the next of `answers` (text when none is left). Every
// request's method and x-api-key are recorded.
class Service : public QObject {
public:
  struct Asked {
    QByteArray method;
    QByteArray key; // empty: no x-api-key header
  };

  Service() {
    m_server.listen(QHostAddress(QHostAddress::LocalHost));
    connect(&m_server, &QTcpServer::newConnection, this, [this] {
      while (QTcpSocket* socket = m_server.nextPendingConnection()) {
        connect(socket, &QTcpSocket::readyRead, this, [this, socket] { read(socket); });
        connect(socket, &QTcpSocket::disconnected, socket, &QObject::deleteLater);
      }
    });
  }

  quint16 port() const { return m_server.serverPort(); }
  QList<Asked> posts() const {
    QList<Asked> found;
    for (const Asked& asked : m_asked) {
      if (asked.method == "POST") {
        found.append(asked);
      }
    }
    return found;
  }
  QList<QByteArray> answers;

private:
  void read(QTcpSocket* socket) {
    QByteArray& input = m_input[socket];
    input += socket->readAll();
    const qsizetype end = input.indexOf("\r\n\r\n");
    if (end < 0) {
      return;
    }
    const QList<QByteArray> lines = input.left(end).split('\n');
    Asked asked;
    asked.method = lines.value(0).split(' ').value(0).trimmed();
    qsizetype length = 0;
    for (qsizetype index = 1; index < lines.size(); ++index) {
      const qsizetype colon = lines.at(index).indexOf(':');
      if (colon < 0) {
        continue;
      }
      const QByteArray name = lines.at(index).left(colon).trimmed().toLower();
      const QByteArray value = lines.at(index).mid(colon + 1).trimmed();
      if (name == "x-api-key") {
        asked.key = value;
      } else if (name == "content-length") {
        length = value.toLongLong();
      }
    }
    if (input.size() < end + 4 + length) {
      return; // the body is still coming
    }
    m_input.remove(socket);
    m_asked.append(asked);
    QByteArray body;
    QByteArray type;
    if (asked.method == "GET") {
      type = "application/json";
      const QJsonObject model{{QStringLiteral("id"), QStringLiteral("claude-opus-5-5")},
                              {QStringLiteral("display_name"), QStringLiteral("Claude Opus 5.5")}};
      body = QJsonDocument(QJsonObject{{QStringLiteral("data"), QJsonArray{model}}}).toJson(QJsonDocument::Compact);
    } else {
      type = "text/event-stream";
      body = answers.isEmpty() ? textAnswer() : answers.takeFirst();
    }
    socket->write(QByteArrayLiteral("HTTP/1.1 200 OK\r\nContent-Type: ") + type +
                  QByteArrayLiteral("\r\nConnection: close\r\nContent-Length: ") + QByteArray::number(body.size()) +
                  QByteArrayLiteral("\r\n\r\n") + body);
    socket->disconnectFromHost();
  }

  QTcpServer m_server;
  QHash<QTcpSocket*, QByteArray> m_input;
  QList<Asked> m_asked;
};

int turnEnds(const QList<QJsonObject>& events) {
  int count = 0;
  for (const QJsonObject& item : events) {
    if (item.value(QStringLiteral("kind")).toString() == QStringLiteral("turn_end")) {
      ++count;
    }
  }
  return count;
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication application(argc, argv);
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("api_agent"));
  QTemporaryDir directory;
  if (!directory.isValid()) {
    std::fprintf(stderr, "no temporary folder\n");
    return 1;
  }
  // The run's own settings, keys, chat folder and listener records.
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.filePath(QStringLiteral("settings")));
  qputenv("MITCAD_TEST_KEYCHAIN", "off");
  qunsetenv("ANTHROPIC_API_KEY");
  qputenv("MITCAD_CHAT_DIR", directory.filePath(QStringLiteral("chat")).toUtf8());
  qputenv("MITCAD_MCP_DISCOVERY_DIR", directory.filePath(QStringLiteral("discovery")).toUtf8());
  QNetworkProxy::setApplicationProxy(QNetworkProxy(QNetworkProxy::NoProxy));

  Listener listener;
  Service service;
  mitcad::mcp::Endpoint record;
  record.instance = QStringLiteral("0123456789abcdef0123456789abcdef");
  record.token = QString(64, QLatin1Char('a'));
  record.pid = QCoreApplication::applicationPid();
  record.port = listener.port();
  QString error;
  if (listener.port() == 0 || service.port() == 0 || !mitcad::mcp::publishEndpoint(record, &error)) {
    std::fprintf(stderr, "set-up failed: %s\n", qPrintable(error));
    return 1;
  }
  mitcad::agent::setApiAgentEnabled(QStringLiteral("anthropic"), true);
  mitcad::agent::setApiAgentUrl(QStringLiteral("anthropic"), QStringLiteral("http://127.0.0.1:%1").arg(service.port()));

  ApiAgent::Preset preset;
  for (const ApiAgent::Preset& candidate : ApiAgent::presets()) {
    if (candidate.id == QStringLiteral("anthropic")) {
      preset = candidate;
    }
  }
  CHECK(preset.id == QStringLiteral("anthropic"));
  ApiAgent agent(preset);
  QList<QJsonObject> events;
  QObject::connect(&agent, &ChatAgent::panelEvent, &agent, [&events](const QJsonObject& item) { events.append(item); });
  QString ended;
  QObject::connect(&agent, &ChatAgent::exited, &agent,
                   [&ended](int, const QString& errors, bool) { ended = errors.isEmpty() ? QStringLiteral("?") : errors; });

  // The key given in the panel: kept for the run, the service asked.
  agent.setKey(kKey);
  CHECK(waitFor([&agent] { return agent.readiness() == ChatAgent::Readiness::Ready; }));

  ChatAgent::Start start;
  start.session = QStringLiteral("session-1309");
  start.instance = record.instance;
  start.chat = QStringLiteral("chat-1309");
  CHECK(agent.start(start, &error));
  CHECK(waitFor([&listener] { return listener.authenticated(); }));

  // A message whose answer calls a tool; Preferences' signal while the call
  // runs (an approval card is open), then its result.
  service.answers = {callAnswer(), textAnswer()};
  ChatAgent::Message first;
  first.text = QStringLiteral("Measure the bodies.");
  CHECK(agent.send(first, &error));
  CHECK(waitFor([&listener] { return listener.calls().size() == 1; }));
  emit ChatNotifier::instance().apiSettingsChanged();
  if (!listener.calls().isEmpty()) {
    listener.answer(listener.calls().first());
  }
  CHECK(waitFor([&events] { return turnEnds(events) == 1; }));
  QList<Service::Asked> posts = service.posts();
  CHECK(posts.size() == 2);
  CHECK(posts.value(0).key == kKey.toUtf8());
  CHECK(posts.value(1).key == kKey.toUtf8()); // the message's next request

  // The next message's first request.
  ChatAgent::Message second;
  second.text = QStringLiteral("Thanks.");
  CHECK(agent.send(second, &error));
  CHECK(waitFor([&events] { return turnEnds(events) == 2; }));
  posts = service.posts();
  CHECK(posts.size() == 3);
  CHECK(posts.value(2).key == kKey.toUtf8());
  CHECK(ended.isEmpty());

  // The key removed in Preferences: the next message ends the conversation
  // with a notice; no request goes without the key.
  mitcad::agent::removeApiKey(QStringLiteral("anthropic"), &agent, [](const QString&) {});
  emit ChatNotifier::instance().apiSettingsChanged();
  ChatAgent::Message third;
  third.text = QStringLiteral("Once more.");
  CHECK(agent.send(third, &error));
  CHECK(waitFor([&ended] { return !ended.isEmpty(); }));
  CHECK(ended.contains(QStringLiteral("removed")));
  CHECK(!agent.running());
  CHECK(service.posts().size() == 3);

  mitcad::mcp::removeEndpoint(record);
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("API agent key tests passed\n");
  return 0;
}
