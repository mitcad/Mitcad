// SPDX-License-Identifier: MIT
#include "mcp-connect.hpp"

#include <algorithm>
#include <condition_variable>
#include <cstdio>
#include <deque>
#include <exception>
#include <iostream>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QStringList>
#include <QTcpSocket>
#include <QThread>
#include <QTimer>

#ifdef Q_OS_WIN
#include <QWinEventNotifier>
#include <qt_windows.h>

#include "update/RunningPrograms.hpp"
#else
#include <cerrno>
#include <unistd.h>
#endif

#include "mcp/LocalEndpoint.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad::cli {
namespace {

using mcp::kMaxFrameBytes;
constexpr int kConnectTimeoutMs = 5000;

int connectionError(const QString& message) {
  std::cerr << "mitcad-cli mcp: " << message.toStdString() << '\n';
  return 1;
}

bool authenticate(QTcpSocket& socket, const mcp::Endpoint& endpoint, int timeoutMs, const QString& chat = QString()) {
  socket.setReadBufferSize(kMaxFrameBytes + 1);
  QElapsedTimer waiting;
  waiting.start();
  // A connection the listener closed before it answered (it was among the
  // oldest waiting ones while new connections came, mitcad#935) is opened
  // again until the time is up; an answer, also a refusal, is final.
  for (;;) {
    const qint64 left = timeoutMs - waiting.elapsed();
    if (left <= 0) {
      return false;
    }
    socket.abort();
    socket.connectToHost(QStringLiteral("127.0.0.1"), endpoint.port);
    if (!socket.waitForConnected(static_cast<int>(left))) {
      return false;
    }
    const QByteArray authentication = mcp::encodeAuth(endpoint, chat);
    if (socket.write(authentication) != authentication.size()) {
      return false;
    }
    bool closed = false;
    while (!socket.canReadLine()) {
      const qint64 remaining = timeoutMs - waiting.elapsed();
      if (remaining <= 0 || socket.bytesAvailable() > kMaxFrameBytes) {
        return false;
      }
      if (!socket.waitForReadyRead(static_cast<int>(remaining))) {
        closed = socket.state() != QAbstractSocket::ConnectedState;
        break;
      }
    }
    if (closed) {
      continue;
    }
    if (!socket.canReadLine()) {
      return false;
    }
    const QJsonDocument answer = QJsonDocument::fromJson(socket.readLine(kMaxFrameBytes + 1));
    return answer.isObject() && answer.object().value(QStringLiteral("authenticated")) == QJsonValue(true);
  }
}

// Private records can survive a crash. Only the instance owning the
// record's token may authenticate it; a reused port is not sufficient.
// discover() has dropped the records of processes no longer running; the
// rest are probed in parallel, each on its own thread with its own socket,
// so that unanswered records cost one timeout in all, not one each
// (mitcad#437).
QList<mcp::Endpoint> liveEndpoints(QString* error, int timeoutMs = 500) {
  const QList<mcp::Endpoint> records = mcp::discover(error);
  std::vector<int> live(static_cast<std::size_t>(records.size()), 0);
  // QThreads, not std::threads: a socket's notifiers need the thread's
  // event dispatcher, even when only its blocking calls are used.
  std::vector<std::unique_ptr<QThread>> probes;
  probes.reserve(live.size());
  for (std::size_t index = 0; index < live.size(); ++index) {
    probes.emplace_back(QThread::create([&records, &live, index, timeoutMs] {
      QTcpSocket probe;
      live[index] = authenticate(probe, records[static_cast<qsizetype>(index)], timeoutMs) ? 1 : 0;
    }));
    probes.back()->start();
  }
  for (const std::unique_ptr<QThread>& probe : probes) {
    probe->wait();
  }
  QList<mcp::Endpoint> endpoints;
  for (std::size_t index = 0; index < live.size(); ++index) {
    if (live[index] != 0) {
      endpoints.append(records[static_cast<qsizetype>(index)]);
    }
  }
  return endpoints;
}

struct Input {
  std::mutex mutex;
  std::condition_variable room;
  std::deque<QByteArray> frames;
  qsizetype bytes = 0;
  bool ended = false;
  bool stop = false;
  QString error;
};

// The reader never touches Qt objects on the event-loop thread. Its queue
// and one incomplete frame are bounded, including when the peer is slow.
qint64 readStdin(char* bytes, std::size_t size) {
#ifdef Q_OS_WIN
  DWORD count = 0;
  if (ReadFile(GetStdHandle(STD_INPUT_HANDLE), bytes, static_cast<DWORD>(size), &count, nullptr)) {
    return count;
  }
  const DWORD error = GetLastError();
  return error == ERROR_BROKEN_PIPE || error == ERROR_HANDLE_EOF ? 0 : -1;
#else
  ssize_t count;
  do {
    count = ::read(STDIN_FILENO, bytes, size);
  } while (count < 0 && errno == EINTR);
  return count;
#endif
}

void readInput(const std::shared_ptr<Input>& input) {
  QByteArray frame;
  // A client's writer may start its stream with a UTF-8 byte order mark
  // (Windows PowerShell's process input with a UTF-8 console code page,
  // mitcad#265): not part of the first message, so it is dropped there.
  bool first = true;
  const auto publish = [&] {
    if (first) {
      first = false;
      if (frame.startsWith(QByteArrayLiteral("\xEF\xBB\xBF"))) {
        frame.remove(0, 3);
      }
    }
    frame.append('\n');
    std::unique_lock<std::mutex> lock(input->mutex);
    input->room.wait(lock, [&] { return input->stop || input->bytes + frame.size() <= kMaxFrameBytes + 1; });
    if (input->stop) {
      return false;
    }
    input->bytes += frame.size();
    input->frames.push_back(std::move(frame));
    frame.clear();
    return true;
  };
  char bytes[4096];
  for (;;) {
    const qint64 count = readStdin(bytes, sizeof(bytes));
    if (count <= 0) {
      if (count == 0 && !frame.isEmpty()) {
        publish();
      }
      std::lock_guard<std::mutex> lock(input->mutex);
      if (count < 0) {
        input->error = QStringLiteral("Could not read MCP stdin");
      }
      input->ended = true;
      return;
    }
    for (qint64 index = 0; index < count; ++index) {
      if (bytes[index] == '\n') {
        if (publish()) {
          continue;
        }
        std::lock_guard<std::mutex> lock(input->mutex);
        input->ended = true;
        return;
      }
      if (frame.size() == kMaxFrameBytes) {
        std::lock_guard<std::mutex> lock(input->mutex);
        input->error = QStringLiteral("Message exceeds the 8 MiB input limit");
        input->ended = true;
        return;
      }
      frame.append(bytes[index]);
    }
  }
}

bool expectsResponse(const QByteArray& frame) {
  if (QString::fromUtf8(frame).toUtf8() != frame) {
    return true;
  }
  const QJsonDocument message = QJsonDocument::fromJson(frame);
  if (!message.isObject()) {
    return true;
  }
  const QJsonObject object = message.object();
  if (object.value(QStringLiteral("jsonrpc")) != QStringLiteral("2.0")) {
    return true;
  }
  if (!object.contains(QStringLiteral("method")) &&
      (object.contains(QStringLiteral("result")) || object.contains(QStringLiteral("error")))) {
    // Client replies are valid protocol envelopes, not another request.
    return false;
  }
  return object.contains(QStringLiteral("id")) || !object.value(QStringLiteral("method")).isString();
}

} // namespace

int mcpListInstances() {
  int argc = 1;
  char name[] = "mitcad-cli";
  char* argv[] = {name, nullptr};
  QCoreApplication app(argc, argv);
  QString error;
  const QList<mcp::Endpoint> endpoints = liveEndpoints(&error);
  if (!error.isEmpty()) {
    return connectionError(error);
  }
  QJsonArray instances;
  for (const mcp::Endpoint& endpoint : endpoints) {
    instances.append(QJsonObject{{QStringLiteral("instance"), endpoint.instance},
                                 {QStringLiteral("pid"), endpoint.pid},
                                 {QStringLiteral("host"), QStringLiteral("127.0.0.1")},
                                 {QStringLiteral("port"), endpoint.port}});
  }
  const QByteArray output = QJsonDocument(QJsonObject{{QStringLiteral("instances"), instances}})
                                .toJson(QJsonDocument::Compact);
  std::cout << output.constData() << '\n';
  return 0;
}

namespace {

// What the bridge answers a tool call while no window is connected
// (mitcad#1288), and a call whose window connection ended before it was
// answered.
const QString kNotRunning = QStringLiteral("MitCad is not running: start MitCad (or reopen the design) and try again");
const QString kEndedDuringCall =
    QStringLiteral("The connection to MitCad ended during this call: check the design in MitCad, then try again");
// Looking for a window again while none is connected; after a window refused
// this client (its assistant was disconnected there, mitcad#1289), less often.
constexpr int kRetryMs = 2000;
constexpr int kRefusedRetryMs = 10000;
// A tool call looks again at once, at most this often.
constexpr int kCallRetryMs = 250;
// The probes of a later look: a busy window answers within it.
constexpr int kProbeMs = 1500;

QByteArray idKey(const QJsonValue& id) { return QJsonDocument(QJsonArray{id}).toJson(QJsonDocument::Compact); }

QByteArray compact(const QJsonObject& object) { return QJsonDocument(object).toJson(QJsonDocument::Compact); }

QByteArray protocolError(int code, const QString& message) {
  return compact({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
                  {QStringLiteral("id"), QJsonValue()},
                  {QStringLiteral("error"), QJsonObject{{QStringLiteral("code"), code}, {QStringLiteral("message"), message}}}}) +
         '\n';
}

QString windowLabel(const mcp::Endpoint& endpoint) {
  QString title = endpoint.title;
  for (qsizetype index = 0; index < title.size(); ++index) {
    if (!title.at(index).isPrint()) title[index] = QChar(' ');
  }
  return QStringLiteral("%1 (%2)").arg(endpoint.instance, title);
}

struct Attempt {
  enum class Kind { Connected, NotRunning, Ambiguous, Failed };
  Kind kind = Kind::NotRunning;
  // What to tell: the choice among several windows when connected, else why not.
  QString message;
};

// The named instance, else the window active last; `socket` is
// authenticated to it when Connected.
Attempt connectWindow(QTcpSocket& socket, const QString& requested, const QString& chat, int timeoutMs) {
  QString error;
  if (requested.isEmpty()) {
    // Connecting allows the GUI to finish a busy event-loop turn; a quick
    // listing probe must not classify that live instance as absent.
    const QList<mcp::Endpoint> endpoints = liveEndpoints(&error, timeoutMs);
    if (!error.isEmpty()) {
      return {Attempt::Kind::Failed, error};
    }
    if (endpoints.isEmpty()) {
      return {Attempt::Kind::NotRunning, kNotRunning};
    }
    const auto chosen = std::max_element(endpoints.cbegin(), endpoints.cend(),
        [](const mcp::Endpoint& a, const mcp::Endpoint& b) { return a.activatedAt < b.activatedAt; });
    if (endpoints.size() > 1 && chosen->activatedAt == 0) {
      return {Attempt::Kind::Ambiguous,
              QStringLiteral("Several MitCad instances are running; use --list-instances and --instance ID")};
    }
    if (!authenticate(socket, *chosen, timeoutMs, chat)) {
      return {Attempt::Kind::Failed, QStringLiteral("MitCad refused authentication; rediscover its running instances")};
    }
    if (endpoints.size() == 1) {
      return {Attempt::Kind::Connected, {}};
    }
    QStringList others;
    for (const mcp::Endpoint& endpoint : endpoints) {
      if (endpoint.instance != chosen->instance) others << windowLabel(endpoint);
    }
    return {Attempt::Kind::Connected, QStringLiteral("connected to %1, the window active last; others: %2")
                                          .arg(windowLabel(*chosen), others.join(QStringLiteral(", ")))};
  }
  // Only the named record is probed (mitcad#437), and the connection that
  // authenticates it is the bridge's.
  const QList<mcp::Endpoint> records = mcp::discover(&error);
  if (!error.isEmpty()) {
    return {Attempt::Kind::Failed, error};
  }
  const auto named = std::find_if(records.cbegin(), records.cend(),
                                  [&](const mcp::Endpoint& record) { return record.instance == requested; });
  if (named == records.cend() || !authenticate(socket, *named, timeoutMs, chat)) {
    return {Attempt::Kind::NotRunning, QStringLiteral("No running MitCad instance '%1'").arg(requested)};
  }
  return {Attempt::Kind::Connected, {}};
}

// The bridge between the client's stdio and a window (mitcad#1288). Without
// --instance it outlives its window's connection: the window restarting its
// endpoint, Disconnect, the window closing, or MitCad not started yet. Then
// a protocol session of its own answers the client as the window would
// (initialize, the tool list, the reference), each other tool call with
// kNotRunning, and the bridge looks for a window again; once one is there,
// the client's initialize, initialized and subscriptions go to it first, and
// the client's messages go on to it. With --instance (the chat panel's own
// agent, or a command copied for one window) the bridge ends with its
// window's connection, as before.
class Bridge {
public:
  Bridge(QCoreApplication& app, QString requested, QString chat)
      : m_app(app), m_requested(std::move(requested)), m_chat(std::move(chat)), m_input(std::make_shared<Input>()) {}

  int run() {
    const Attempt first = connectWindow(m_socket, m_requested, m_chat, kConnectTimeoutMs);
    if (first.kind == Attempt::Kind::Connected) {
      if (!first.message.isEmpty()) std::cerr << "mitcad-cli mcp: " << first.message.toStdString() << '\n';
      m_online = true;
    } else if (follows() && first.kind == Attempt::Kind::NotRunning) {
      std::cerr << "mitcad-cli mcp: MitCad is not running; waiting for it\n";
      startOffline(true);
      m_since.start();
    } else {
      return connectionError(first.message);
    }
    QObject::connect(&m_socket, &QTcpSocket::readyRead, &m_app, [this] { readWindow(); });
    QObject::connect(&m_socket, &QTcpSocket::disconnected, &m_app, [this] {
      if (!m_online) return; // a probe's own abort while no window is connected
      if (!follows()) {
        fail(QStringLiteral("The MitCad instance disconnected"));
        return;
      }
      goOffline(kNotRunning, kRetryMs);
    });
#ifdef Q_OS_WIN
    // An update asks the bridge to end (mitcad#1454): the updater and the
    // installer set this event of the bridges that run from the
    // installation (update/RunningPrograms.hpp), whose files they are about
    // to replace. Manual reset: it stays set until every handle is closed.
    const HANDLE quitEvent = CreateEventW(nullptr, TRUE, FALSE,
                                          mitcad::installation::bridgeQuitEventName(GetCurrentProcessId()).c_str());
    std::unique_ptr<QWinEventNotifier> quitNotifier;
    if (quitEvent != nullptr) {
      quitNotifier = std::make_unique<QWinEventNotifier>(quitEvent);
      QObject::connect(quitNotifier.get(), &QWinEventNotifier::activated, &m_app, [this] {
        end("MitCad is being updated; the bridge ends so that the update can replace its files "
            "(reconnect MitCad in the assistant after the update)");
      });
    }
#endif
    std::thread reader(readInput, m_input);
    QTimer pumpTimer;
    QObject::connect(&pumpTimer, &QTimer::timeout, &m_app, [this] { pump(); });
    pumpTimer.start(5);
    m_app.exec();
    pumpTimer.stop();
#ifdef Q_OS_WIN
    quitNotifier.reset();
    if (quitEvent != nullptr) {
      CloseHandle(quitEvent);
    }
#endif
    QObject::disconnect(&m_socket, nullptr, &m_app, nullptr);
    m_online = false;
    m_socket.abort();
    {
      std::lock_guard<std::mutex> lock(m_input->mutex);
      m_input->stop = true;
      m_input->room.notify_one();
      if (m_input->ended) {
        reader.join();
      } else {
        // Raw OS reads hold no stdio FILE lock during process-exit cleanup.
        // A disconnected GUI must not wait for a client holding stdin open;
        // only shared input state survives until this CLI process exits.
        reader.detach();
      }
    }
    return m_result;
  }

private:
  bool follows() const { return m_requested.isEmpty(); }

  void fail(const QString& message) {
    if (m_result == 0) {
      m_result = connectionError(message);
    }
    m_stopping = true;
    m_app.quit();
  }

#ifdef Q_OS_WIN
  // Ends the bridge without an error (mitcad#1454): an update asked it to.
  void end(const char* reason) {
    if (m_stopping) return;
    std::cerr << "mitcad-cli mcp: " << reason << '\n';
    m_stopping = true;
    m_app.quit();
  }
#endif

  bool emitLine(const QByteArray& line) {
    if (std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stdout) !=
            static_cast<std::size_t>(line.size()) || std::fflush(stdout) != 0) {
      fail(QStringLiteral("Could not write MCP stdout"));
      return false;
    }
    return true;
  }

  // What a window connected later has to be told first.
  void observe(const QJsonObject& message) {
    if (message.value(QStringLiteral("jsonrpc")) != QStringLiteral("2.0")) return;
    const QString method = message.value(QStringLiteral("method")).toString();
    const bool request = message.contains(QStringLiteral("id"));
    if (method == QStringLiteral("initialize") && request) {
      m_initialize = message;
    } else if (method == QStringLiteral("notifications/initialized") && !request) {
      m_initialized = true;
    } else if ((method == QStringLiteral("resources/subscribe") || method == QStringLiteral("resources/unsubscribe")) &&
               request) {
      const QString uri = message.value(QStringLiteral("params")).toObject().value(QStringLiteral("uri")).toString();
      m_subscriptions.removeAll(uri);
      if (!uri.isEmpty() && method == QStringLiteral("resources/subscribe")) m_subscriptions << uri;
    }
  }

  QJsonObject internalRequest(const QString& method, const QJsonObject& params) {
    const QString id = QStringLiteral("mitcad-bridge-%1").arg(++m_internalCount);
    return {{QStringLiteral("jsonrpc"), QStringLiteral("2.0")}, {QStringLiteral("id"), id},
            {QStringLiteral("method"), method}, {QStringLiteral("params"), params}};
  }

  QJsonObject replayedInitialize() {
    QJsonObject replay = m_initialize;
    replay.insert(QStringLiteral("id"), QStringLiteral("mitcad-bridge-%1").arg(++m_internalCount));
    return replay;
  }

  static QJsonObject initializedNotification() {
    return {{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
            {QStringLiteral("method"), QStringLiteral("notifications/initialized")}};
  }

  // A message to the bridge's own session whose answer nobody waits for.
  void feedOffline(const QJsonObject& message) {
    try {
      const QByteArray text = compact(message);
      (void)(*m_offline)->prepare_mcp(rust::Str(text.constData(), static_cast<std::size_t>(text.size())));
    } catch (const std::exception&) {
    }
  }

  // The bridge's own session, in the state the client's messages left it.
  void startOffline(bool withInitialize) {
    m_offline.emplace(mitcad::new_live_mcp_session());
    if (withInitialize && !m_initialize.isEmpty()) {
      feedOffline(replayedInitialize());
      if (m_initialized) feedOffline(initializedNotification());
      for (const QString& uri : m_subscriptions) {
        feedOffline(internalRequest(QStringLiteral("resources/subscribe"), {{QStringLiteral("uri"), uri}}));
      }
    }
  }

  void answerOffline(const QByteArray& frame, const QString& toolError) {
    QByteArray text = frame;
    if (text.endsWith('\n')) text.chop(1);
    try {
      const std::string planText(
          (*m_offline)->prepare_mcp(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
      const QJsonObject plan = QJsonDocument::fromJson(QByteArray::fromStdString(planText)).object();
      if (plan.contains(QStringLiteral("response"))) {
        const QJsonValue response = plan.value(QStringLiteral("response"));
        if (response.isString()) emitLine(response.toString().toUtf8() + '\n');
        return;
      }
      const QByteArray error = compact({{QStringLiteral("error"), toolError}});
      const std::string answer(
          (*m_offline)->finish_mcp(rust::Str(error.constData(), static_cast<std::size_t>(error.size())), true));
      emitLine(QByteArray::fromStdString(answer) + '\n');
    } catch (const std::exception& failure) {
      emitLine(protocolError(-32603, QString::fromUtf8(failure.what())));
    }
  }

  bool sendWindow(const QByteArray& frame) {
    if (m_socket.write(frame) != frame.size()) {
      goOffline(kNotRunning, kRetryMs);
      return false;
    }
    return true;
  }

  void goOnline(const QString& note) {
    m_online = true;
    m_response.clear();
    m_internal.clear();
    m_offline.reset();
    std::cerr << "mitcad-cli mcp: connected to MitCad again"
              << (note.isEmpty() ? std::string() : "; " + note.toStdString()) << '\n';
    if (m_initialize.isEmpty()) return;
    const QJsonObject initialize = replayedInitialize();
    m_internal.insert(idKey(initialize.value(QStringLiteral("id"))), QStringLiteral("initialize"));
    if (!sendWindow(compact(initialize) + '\n')) return;
    if (m_initialized && !sendWindow(compact(initializedNotification()) + '\n')) return;
    for (const QString& uri : m_subscriptions) {
      const QJsonObject subscribe = internalRequest(QStringLiteral("resources/subscribe"), {{QStringLiteral("uri"), uri}});
      m_internal.insert(idKey(subscribe.value(QStringLiteral("id"))), QStringLiteral("resources/subscribe"));
      if (!sendWindow(compact(subscribe) + '\n')) return;
    }
  }

  void goOffline(const QString& reason, int retryMs) {
    if (!m_online) return;
    m_online = false;
    m_socket.abort();
    m_response.clear();
    m_internal.clear();
    m_reason = reason;
    m_retryMs = retryMs;
    m_since.start();
    std::cerr << "mitcad-cli mcp: the connection to MitCad ended; waiting for MitCad (" << reason.toStdString() << ")\n";
    // The calls the window did not answer are answered here; an initialize
    // among them starts the bridge's own session itself.
    const QList<std::pair<QByteArray, QByteArray>> cut = std::exchange(m_forwarded, {});
    m_untracked = 0;
    const bool initializing = std::any_of(cut.cbegin(), cut.cend(), [](const std::pair<QByteArray, QByteArray>& entry) {
      return QJsonDocument::fromJson(entry.second).object().value(QStringLiteral("method")) == QStringLiteral("initialize");
    });
    startOffline(!initializing);
    for (const auto& entry : cut) answerOffline(entry.second, kEndedDuringCall);
    if (initializing) {
      if (m_initialized) feedOffline(initializedNotification());
      for (const QString& uri : m_subscriptions) {
        feedOffline(internalRequest(QStringLiteral("resources/subscribe"), {{QStringLiteral("uri"), uri}}));
      }
    }
  }

  void tryConnect() {
    m_since.start();
    const Attempt attempt = connectWindow(m_socket, m_requested, m_chat, kProbeMs);
    if (attempt.kind == Attempt::Kind::Connected) {
      goOnline(attempt.message);
      return;
    }
    m_socket.abort();
    if (attempt.kind == Attempt::Kind::NotRunning) {
      // A window that refused this client has closed since.
      m_reason = kNotRunning;
      m_retryMs = kRetryMs;
    } else {
      m_reason = attempt.message;
    }
  }

  void forward(const QByteArray& frame, const QJsonDocument& message) {
    if (expectsResponse(frame)) {
      const QJsonObject object = message.object();
      // The window answers an id that is neither a string nor an integer
      // with a null id (core/mcp/src/lib.rs valid_id): such a request is
      // counted, not tracked (mitcad#1317).
      const QJsonValue id = object.value(QStringLiteral("id"));
      const bool usableId =
          id.isString() || (id.isDouble() && static_cast<double>(id.toInteger()) == id.toDouble());
      if (message.isObject() && usableId && object.value(QStringLiteral("method")).isString()) {
        m_forwarded.append({idKey(object.value(QStringLiteral("id"))), frame});
      } else {
        ++m_untracked;
      }
    }
    if (m_socket.write(frame) != frame.size()) {
      if (follows()) goOffline(kNotRunning, kRetryMs);
      else fail(QStringLiteral("Could not send an MCP message to MitCad"));
    }
  }

  void handleFrame(const QByteArray& frame) {
    const QJsonDocument message = QJsonDocument::fromJson(frame);
    if (message.isObject()) observe(message.object());
    // A call right after MitCad started finds it, unless a window refused
    // this client a moment ago.
    if (!m_online && follows() && m_retryMs == kRetryMs && m_since.elapsed() >= kCallRetryMs &&
        message.object().value(QStringLiteral("method")) == QStringLiteral("tools/call")) {
      tryConnect();
    }
    if (m_online) forward(frame, message);
    else answerOffline(frame, m_reason);
  }

  void readWindow() {
    // While no window is connected, a probe's socket reads its own answer.
    while (m_online && m_socket.bytesAvailable() > 0) {
      const QByteArray bytes =
          m_socket.read(qMin<qint64>(m_socket.bytesAvailable(), kMaxFrameBytes + 1 - m_response.size()));
      if (bytes.isEmpty()) {
        fail(QStringLiteral("Message exceeds the 8 MiB response limit"));
        return;
      }
      m_response.append(bytes);
      for (;;) {
        const qsizetype newline = m_response.indexOf('\n');
        if (newline < 0) {
          break;
        }
        const QByteArray line = m_response.left(newline + 1);
        m_response.remove(0, newline + 1);
        const QJsonDocument message = QJsonDocument::fromJson(line);
        if (!message.isObject() || message.object().value(QStringLiteral("jsonrpc")) != QStringLiteral("2.0")) {
          fail(QStringLiteral("MitCad sent an invalid MCP message"));
          return;
        }
        const QJsonObject envelope = message.object();
        if (!envelope.contains(QStringLiteral("method")) &&
            (envelope.contains(QStringLiteral("result")) || envelope.contains(QStringLiteral("error")))) {
          const QByteArray key = idKey(envelope.value(QStringLiteral("id")));
          if (const auto internal = m_internal.constFind(key); internal != m_internal.cend()) {
            const QString method = internal.value();
            m_internal.remove(key);
            if (method == QStringLiteral("initialize") && envelope.contains(QStringLiteral("error"))) {
              // The window refused this client (mitcad#1289): its reason
              // answers the calls until it allows it again.
              const QString reason =
                  envelope.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
              goOffline(reason.isEmpty() ? kNotRunning : reason, kRefusedRetryMs);
              return;
            }
            continue; // the bridge's own request
          }
          const auto answered = std::find_if(m_forwarded.begin(), m_forwarded.end(),
              [&key](const std::pair<QByteArray, QByteArray>& entry) { return entry.first == key; });
          if (answered != m_forwarded.end()) {
            m_forwarded.erase(answered);
          } else if (m_untracked > 0) {
            --m_untracked;
          }
        }
        if (!emitLine(line)) {
          return;
        }
      }
      if (m_response.size() > kMaxFrameBytes) {
        fail(QStringLiteral("Message exceeds the 8 MiB response limit"));
        return;
      }
    }
  }

  void pump() {
    if (m_stopping) return;
    for (;;) {
      QByteArray frame;
      {
        std::lock_guard<std::mutex> lock(m_input->mutex);
        if (!m_input->error.isEmpty()) {
          fail(m_input->error);
          return;
        }
        if (m_input->frames.empty() || (m_online && m_socket.bytesToWrite() >= kMaxFrameBytes)) {
          break;
        }
        frame = std::move(m_input->frames.front());
        m_input->frames.pop_front();
        m_input->bytes -= frame.size();
        m_input->room.notify_one();
      }
      handleFrame(frame);
      if (m_stopping) return;
    }
    if (!m_online && follows() && m_since.elapsed() >= m_retryMs) {
      tryConnect();
    }
    bool ended = false;
    {
      std::lock_guard<std::mutex> lock(m_input->mutex);
      ended = m_input->ended && m_input->frames.empty();
    }
    if (ended && (!m_online || (m_socket.bytesToWrite() == 0 && m_forwarded.isEmpty() && m_untracked == 0))) {
      // EOF belongs to this bridge only, not to the desktop application's
      // document or lifetime. All requested responses have been forwarded.
      m_stopping = true;
      m_app.quit();
    }
  }

  QCoreApplication& m_app;
  const QString m_requested;
  const QString m_chat;
  std::shared_ptr<Input> m_input;
  QTcpSocket m_socket;
  bool m_online = false;
  bool m_stopping = false;
  int m_result = 0;
  QByteArray m_response;
  // The client's requests the window has not answered (id, frame), in
  // order; and those whose id cannot be told (the window answers them with
  // a null id).
  QList<std::pair<QByteArray, QByteArray>> m_forwarded;
  int m_untracked = 0;
  // The bridge's own requests to the window, by id: their answers stay here.
  QHash<QByteArray, QString> m_internal;
  quint64 m_internalCount = 0;
  // The client's initialize request, whether it sent initialized, and the
  // resources it subscribed to.
  QJsonObject m_initialize;
  bool m_initialized = false;
  QStringList m_subscriptions;
  std::optional<rust::Box<mitcad::LiveMcpSession>> m_offline;
  QString m_reason = kNotRunning;
  QElapsedTimer m_since; // since the last look for a window
  int m_retryMs = kRetryMs;
};

} // namespace

int mcpConnect(const std::string& instance, const std::string& chatSession) {
  int argc = 1;
  char name[] = "mitcad-cli";
  char* argv[] = {name, nullptr};
  QCoreApplication app(argc, argv);
  const QString requested = QString::fromUtf8(instance.data(), static_cast<qsizetype>(instance.size()));
  // The chat panel's marker of its own connection (mitcad#947), sent with
  // the token: the window asks its user before some of its calls.
  const QString chat = QString::fromUtf8(chatSession.data(), static_cast<qsizetype>(chatSession.size()));
  Bridge bridge(app, requested, chat);
  return bridge.run();
}

} // namespace mitcad::cli

