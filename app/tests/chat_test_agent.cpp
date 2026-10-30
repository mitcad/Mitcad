// SPDX-License-Identifier: MIT
// A scripted stand-in for Claude Code in the chat panel's UI tests
// (mitcad#947, tools/ui-chat-test.py): no AI. Copied into the test's
// MITCAD_TEST_ASSISTANT_EXECUTABLES folder as `claude`, it answers
// `--version`, `auth status` and `auth login` (signed in once the login ran:
// a file in MITCAD_ASSISTANT_TEST_WORK), and in print mode speaks Claude
// Code's stream-json: it checks the command line the chat gives it, starts
// the MCP server of its --mcp-config (the real mitcad-cli bridge, with the
// chat's marker) and, for each user message, streams an answer and calls
// MitCad's tools by the message's words:
//
//   "box"      two boxes (30 x 20 x 10 mm and 10 mm), two calls; with
//              "slowly" it waits between them for continue.png in the work
//              folder (the UI script's screenshot)
//   "save"     document_save to MITCAD_TEST_CHAT_SAVE_PATH
//   "undo"     model_command undo (a step from before the message)
//   "wait"     streams a line, then waits for its turn to be stopped
//   otherwise  a plain answer naming what came with the message
//
// It records each start's arguments (chat-starts.jsonl) and each message's
// text, context and pictures (chat-messages.jsonl) in the work folder; a
// session it was given once (--session-id) can be resumed (--resume), an
// unknown one cannot. Never installed or linked into the application.
//
// Copied as `codex` it stands in for Codex the same way: `--version`,
// `login status` and `login` (the file codex-signed-in), and `app-server`,
// which speaks Codex's app-server JSON-RPC (core/mcp/src/chat/codex.rs):
// initialize, model/list (two test models), thread/start and thread/resume
// (it checks that MitCad's bridge is the only MCP server and that Codex's
// own tools are off, and starts that bridge), turn/start (the same words as
// above, as agentMessage deltas, mcpToolCall items and turn/completed) and
// turn/interrupt ("wait" ends as interrupted). Its starts record the
// thread requests' parameters (the model among them), its messages the
// turn's model too.
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <csignal>
#include <cstdio>
#include <deque>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <utility>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QStringList>
#include <QUuid>

namespace {

std::atomic<bool> interrupted{false};

#ifndef _WIN32
void onInterrupt(int) { interrupted = true; }
#endif

QString work() { return qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK"); }

void appendLine(const QString& name, const QJsonObject& value) {
  QFile file(QDir(work()).filePath(name));
  if (file.open(QIODevice::WriteOnly | QIODevice::Append)) {
    file.write(QJsonDocument(value).toJson(QJsonDocument::Compact) + '\n');
  }
}

void emitLine(const QJsonObject& message) {
  const QByteArray line = QJsonDocument(message).toJson(QJsonDocument::Compact);
  std::fwrite(line.constData(), 1, static_cast<std::size_t>(line.size()), stdout);
  std::fputc('\n', stdout);
  std::fflush(stdout);
}

class Bridge {
public:
  bool start(const QString& program, const QStringList& arguments, const QString& client = QStringLiteral("claude-code")) {
    m_process.setProgram(program);
    m_process.setArguments(arguments);
    m_process.setProcessChannelMode(QProcess::ForwardedErrorChannel);
    m_process.start();
    if (!m_process.waitForStarted(15000)) {
      return false;
    }
    const QJsonObject initialized =
        call(QStringLiteral("initialize"),
             {{QStringLiteral("protocolVersion"), QStringLiteral("2025-11-25")},
              {QStringLiteral("capabilities"), QJsonObject()},
              {QStringLiteral("clientInfo"),
               QJsonObject{{QStringLiteral("name"), client},
                           {QStringLiteral("version"), QStringLiteral("test")}}}},
             30000);
    if (!initialized.contains(QStringLiteral("result"))) {
      return false;
    }
    write({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
           {QStringLiteral("method"), QStringLiteral("notifications/initialized")}});
    return true;
  }

  // A tool's structured result and whether it is an error; the call waits
  // as long as an approval card does.
  QJsonObject tool(const QString& name, const QJsonObject& arguments, bool* error) {
    const QJsonObject answer = call(QStringLiteral("tools/call"),
                                    {{QStringLiteral("name"), name}, {QStringLiteral("arguments"), arguments}}, 180000);
    const QJsonObject result = answer.value(QStringLiteral("result")).toObject();
    *error = !answer.contains(QStringLiteral("result")) || result.value(QStringLiteral("isError")).toBool();
    QJsonObject structured = result.value(QStringLiteral("structuredContent")).toObject();
    if (!answer.contains(QStringLiteral("result"))) {
      structured.insert(QStringLiteral("error"), answer.value(QStringLiteral("error")).toObject()
                                                     .value(QStringLiteral("message")).toString(QStringLiteral("no answer")));
    }
    return structured;
  }

private:
  void write(const QJsonObject& message) {
    m_process.write(QJsonDocument(message).toJson(QJsonDocument::Compact) + '\n');
    m_process.waitForBytesWritten(5000);
  }

  QJsonObject call(const QString& method, const QJsonObject& params, int timeoutMs) {
    const int id = m_next++;
    write({{QStringLiteral("jsonrpc"), QStringLiteral("2.0")},
           {QStringLiteral("id"), id},
           {QStringLiteral("method"), method},
           {QStringLiteral("params"), params}});
    for (;;) {
      while (!m_process.canReadLine()) {
        if (m_process.state() == QProcess::NotRunning || !m_process.waitForReadyRead(timeoutMs)) {
          return {};
        }
      }
      const QJsonObject reply = QJsonDocument::fromJson(m_process.readLine()).object();
      if (reply.value(QStringLiteral("id")).toInt() == id) {
        return reply;
      }
    }
  }

  QProcess m_process;
  int m_next = 1;
};

class Conversation {
public:
  Conversation(QString session, Bridge& bridge) : m_session(std::move(session)), m_bridge(bridge) {}

  void answer(const QString& text) {
    ++m_messages;
    const QString id = QStringLiteral("msg_%1").arg(m_messages);
    emitLine({{QStringLiteral("type"), QStringLiteral("stream_event")},
              {QStringLiteral("event"), QJsonObject{{QStringLiteral("type"), QStringLiteral("message_start")},
                                                    {QStringLiteral("message"), QJsonObject{{QStringLiteral("id"), id}}}}}});
    // The text in two pieces, then the whole message.
    const qsizetype half = text.size() / 2;
    const QStringList pieces{text.left(half), text.mid(half)};
    for (const QString& piece : pieces) {
      emitLine({{QStringLiteral("type"), QStringLiteral("stream_event")},
                {QStringLiteral("event"),
                 QJsonObject{{QStringLiteral("type"), QStringLiteral("content_block_delta")},
                             {QStringLiteral("index"), 0},
                             {QStringLiteral("delta"), QJsonObject{{QStringLiteral("type"), QStringLiteral("text_delta")},
                                                                   {QStringLiteral("text"), piece}}}}}});
    }
    QJsonArray content;
    content.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("text")}, {QStringLiteral("text"), text}});
    emitLine({{QStringLiteral("type"), QStringLiteral("assistant")},
              {QStringLiteral("message"), QJsonObject{{QStringLiteral("id"), id}, {QStringLiteral("content"), content}}}});
  }

  QJsonObject call(const QString& name, const QJsonObject& arguments, bool* error) {
    ++m_messages;
    const QString use = QStringLiteral("toolu_%1").arg(m_messages);
    QJsonArray content;
    content.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("tool_use")},
                               {QStringLiteral("id"), use},
                               {QStringLiteral("name"), QStringLiteral("mcp__mitcad__") + name},
                               {QStringLiteral("input"), arguments}});
    emitLine({{QStringLiteral("type"), QStringLiteral("assistant")},
              {QStringLiteral("message"), QJsonObject{{QStringLiteral("id"), QStringLiteral("msg_%1").arg(m_messages)},
                                                      {QStringLiteral("content"), content}}}});
    const QJsonObject result = m_bridge.tool(name, arguments, error);
    QJsonArray results;
    results.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("tool_result")},
                               {QStringLiteral("tool_use_id"), use},
                               {QStringLiteral("is_error"), *error},
                               {QStringLiteral("content"), QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact))}});
    emitLine({{QStringLiteral("type"), QStringLiteral("user")},
              {QStringLiteral("message"), QJsonObject{{QStringLiteral("role"), QStringLiteral("user")},
                                                      {QStringLiteral("content"), results}}}});
    return result;
  }

  void end(bool error, const QString& subtype, const QString& text) {
    emitLine({{QStringLiteral("type"), QStringLiteral("result")},
              {QStringLiteral("subtype"), subtype},
              {QStringLiteral("is_error"), error},
              {QStringLiteral("result"), text},
              {QStringLiteral("session_id"), m_session}});
  }

  QString document() {
    bool error = false;
    const QJsonObject listed = m_bridge.tool(QStringLiteral("document_list"), QJsonObject(), &error);
    return listed.value(QStringLiteral("documents")).toArray().at(0).toObject().value(QStringLiteral("document")).toString();
  }

private:
  QString m_session;
  Bridge& m_bridge;
  int m_messages = 0;
};

QJsonObject box(const QString& document, int x, int length, int width, int height) {
  QJsonArray corner;
  corner.append(x);
  corner.append(0);
  return {{QStringLiteral("document"), document},
          {QStringLiteral("command"),
           QJsonObject{{QStringLiteral("cmd"), QStringLiteral("add_feature")},
                       {QStringLiteral("def"), QJsonObject{{QStringLiteral("type"), QStringLiteral("box")},
                                                           {QStringLiteral("plane"), QStringLiteral("xy")},
                                                           {QStringLiteral("corner"), corner},
                                                           {QStringLiteral("length"), length},
                                                           {QStringLiteral("width"), width},
                                                           {QStringLiteral("height"), height},
                                                           {QStringLiteral("operation"), QStringLiteral("new_body")}}}}}};
}

int printMode(const QStringList& args) {
  appendLine(QStringLiteral("chat-starts.jsonl"), {{QStringLiteral("argv"), QJsonArray::fromStringList(args)}});
  const auto value = [&args](const QString& option) {
    const qsizetype at = args.indexOf(option);
    return at >= 0 && at + 1 < args.size() ? args.at(at + 1) : QString();
  };
  // The command line the chat must give: stream-json both ways, no
  // built-in tools, only its own MCP server, MitCad's tools allowed.
  const qsizetype tools = args.indexOf(QStringLiteral("--tools"));
  if (value(QStringLiteral("--input-format")) != QStringLiteral("stream-json") ||
      value(QStringLiteral("--output-format")) != QStringLiteral("stream-json") || tools < 0 ||
      tools + 1 >= args.size() || !args.at(tools + 1).isEmpty() || !args.contains(QStringLiteral("--strict-mcp-config")) ||
      value(QStringLiteral("--allowedTools")) != QStringLiteral("mcp__mitcad") ||
      !args.contains(QStringLiteral("--include-partial-messages"))) {
    std::fputs("test agent: unexpected command line\n", stderr);
    return 3;
  }
  QString session = value(QStringLiteral("--session-id"));
  const QString resumed = value(QStringLiteral("--resume"));
  const QDir sessions(QDir(work()).filePath(QStringLiteral("sessions")));
  QDir().mkpath(sessions.path());
  if (!resumed.isEmpty()) {
    if (!QFileInfo::exists(sessions.filePath(resumed))) {
      std::fputs("No conversation found with session ID\n", stderr);
      return 1;
    }
    session = resumed;
  }
  const QJsonObject config = QJsonDocument::fromJson(value(QStringLiteral("--mcp-config")).toUtf8()).object();
  const QJsonObject server = config.value(QStringLiteral("mcpServers")).toObject().value(QStringLiteral("mitcad")).toObject();
  QStringList bridgeArguments;
  for (const QJsonValue& argument : server.value(QStringLiteral("args")).toArray()) {
    bridgeArguments << argument.toString();
  }
  Bridge bridge;
  const bool connected = bridgeArguments.contains(QStringLiteral("--chat-session")) &&
                         bridge.start(server.value(QStringLiteral("command")).toString(), bridgeArguments);
  QJsonArray servers;
  servers.append(QJsonObject{{QStringLiteral("name"), QStringLiteral("mitcad")},
                             {QStringLiteral("status"), connected ? QStringLiteral("connected") : QStringLiteral("failed")}});
  emitLine({{QStringLiteral("type"), QStringLiteral("system")},
            {QStringLiteral("subtype"), QStringLiteral("init")},
            {QStringLiteral("session_id"), session},
            {QStringLiteral("model"), QStringLiteral("test-model")},
            {QStringLiteral("mcp_servers"), servers}});
  Conversation conversation(session, bridge);
  std::string line;
  while (std::getline(std::cin, line)) {
    const QJsonObject message = QJsonDocument::fromJson(QByteArray::fromStdString(line)).object();
    if (message.value(QStringLiteral("type")).toString() != QStringLiteral("user")) {
      continue;
    }
    QFile known(sessions.filePath(session));
    if (!known.exists() && known.open(QIODevice::WriteOnly)) {
      known.close();
    }
    QString text;
    QString context;
    QJsonArray pictures;
    const QJsonArray blocks = message.value(QStringLiteral("message")).toObject().value(QStringLiteral("content")).toArray();
    for (const QJsonValue& block : blocks) {
      const QJsonObject part = block.toObject();
      if (part.value(QStringLiteral("type")).toString() == QStringLiteral("text")) {
        (text.isEmpty() ? text : context) = part.value(QStringLiteral("text")).toString();
      } else if (part.value(QStringLiteral("type")).toString() == QStringLiteral("image")) {
        pictures.append(part.value(QStringLiteral("source")).toObject().value(QStringLiteral("media_type")));
      }
    }
    appendLine(QStringLiteral("chat-messages.jsonl"), {{QStringLiteral("session"), session},
                                                       {QStringLiteral("text"), text},
                                                       {QStringLiteral("context"), context},
                                                       {QStringLiteral("pictures"), pictures}});
    if (!connected) {
      conversation.answer(QStringLiteral("MitCad's tools are not connected."));
      conversation.end(true, QStringLiteral("error_during_execution"), QStringLiteral("MitCad's tools are not connected."));
      continue;
    }
    bool error = false;
    if (text.contains(QStringLiteral("box"))) {
      conversation.answer(QStringLiteral("Adding the boxes."));
      const QString document = conversation.document();
      conversation.call(QStringLiteral("model_command"), box(document, 0, 20, 30, 10), &error);
      if (text.contains(QStringLiteral("slowly"))) {
        const QString signal = QDir(work()).filePath(QStringLiteral("continue.png"));
        for (int waited = 0; waited < 600 && !QFileInfo::exists(signal); ++waited) {
          std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
      }
      conversation.call(QStringLiteral("model_command"), box(document, 40, 10, 10, 10), &error);
      conversation.answer(QStringLiteral("Two boxes are in the design."));
      conversation.end(false, QStringLiteral("success"), QStringLiteral("Two boxes are in the design."));
    } else if (text.contains(QStringLiteral("save"))) {
      conversation.answer(QStringLiteral("Saving the design."));
      const QString document = conversation.document();
      conversation.call(QStringLiteral("document_save"),
                        {{QStringLiteral("document"), document},
                         {QStringLiteral("path"), qEnvironmentVariable("MITCAD_TEST_CHAT_SAVE_PATH")},
                         {QStringLiteral("overwrite"), true}},
                        &error);
      conversation.answer(error ? QStringLiteral("The design was not saved.") : QStringLiteral("Saved."));
      conversation.end(false, QStringLiteral("success"), QString());
    } else if (text.contains(QStringLiteral("undo"))) {
      const QString document = conversation.document();
      conversation.call(QStringLiteral("model_command"),
                        {{QStringLiteral("document"), document},
                         {QStringLiteral("command"), QJsonObject{{QStringLiteral("cmd"), QStringLiteral("undo")}}}},
                        &error);
      conversation.answer(error ? QStringLiteral("The undo was not allowed.") : QStringLiteral("Undone."));
      conversation.end(false, QStringLiteral("success"), QString());
    } else if (text.contains(QStringLiteral("wait"))) {
      conversation.answer(QStringLiteral("Waiting to be stopped."));
      interrupted = false;
      for (int waited = 0; waited < 1200 && !interrupted; ++waited) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
      }
      conversation.end(true, QStringLiteral("error_during_execution"), QStringLiteral("Interrupted by the user."));
    } else {
      const QString reply = QStringLiteral("Hello from the test agent: %1 characters of context, %2 picture(s).")
                                .arg(context.size())
                                .arg(pictures.size());
      conversation.answer(reply);
      conversation.end(false, QStringLiteral("success"), reply);
    }
  }
  return 0;
}

// Codex's app server: stdin read on a thread of its own, so that a
// turn/interrupt arrives while a turn runs.
class Inbox {
public:
  Inbox() : m_reader([this] { read(); }) {}
  ~Inbox() { m_reader.detach(); }
  Inbox(const Inbox&) = delete;
  Inbox& operator=(const Inbox&) = delete;

  // The next message; false at the end of the input.
  bool next(QJsonObject* message) {
    std::unique_lock<std::mutex> lock(m_mutex);
    m_ready.wait(lock, [this] { return !m_lines.empty() || m_closed; });
    if (m_lines.empty()) {
      return false;
    }
    *message = QJsonDocument::fromJson(QByteArray::fromStdString(m_lines.front())).object();
    m_lines.pop_front();
    return true;
  }

  // A turn/interrupt request waiting, taken out (its id); waits up to `ms`.
  bool takeInterrupt(int ms, QJsonValue* id) {
    std::unique_lock<std::mutex> lock(m_mutex);
    m_ready.wait_for(lock, std::chrono::milliseconds(ms));
    for (auto line = m_lines.begin(); line != m_lines.end(); ++line) {
      const QJsonObject request = QJsonDocument::fromJson(QByteArray::fromStdString(*line)).object();
      if (request.value(QStringLiteral("method")).toString() == QStringLiteral("turn/interrupt")) {
        *id = request.value(QStringLiteral("id"));
        m_lines.erase(line);
        return true;
      }
    }
    return false;
  }

private:
  void read() {
    std::string line;
    while (std::getline(std::cin, line)) {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_lines.push_back(line);
      m_ready.notify_all();
    }
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_closed = true;
    m_ready.notify_all();
  }

  std::mutex m_mutex;
  std::condition_variable m_ready;
  std::deque<std::string> m_lines;
  bool m_closed = false;
  std::thread m_reader; // last: it starts once the rest is there
};

void reply(const QJsonValue& id, const QJsonObject& result) {
  emitLine({{QStringLiteral("id"), id}, {QStringLiteral("result"), result}});
}

void replyError(const QJsonValue& id, const QString& message) {
  emitLine({{QStringLiteral("id"), id},
            {QStringLiteral("error"), QJsonObject{{QStringLiteral("code"), -32600}, {QStringLiteral("message"), message}}}});
}

void notify(const QString& method, const QJsonObject& params) {
  emitLine({{QStringLiteral("method"), method}, {QStringLiteral("params"), params}});
}

class CodexTurn {
public:
  CodexTurn(QString thread, QString turn, Bridge& bridge)
      : m_thread(std::move(thread)), m_turn(std::move(turn)), m_bridge(bridge) {}

  void answer(const QString& text) {
    ++m_items;
    const QString item = QStringLiteral("msg_%1").arg(m_items);
    const qsizetype half = text.size() / 2;
    const QStringList pieces{text.left(half), text.mid(half)};
    for (const QString& piece : pieces) {
      notify(QStringLiteral("item/agentMessage/delta"), {{QStringLiteral("threadId"), m_thread},
                                                        {QStringLiteral("turnId"), m_turn},
                                                        {QStringLiteral("itemId"), item},
                                                        {QStringLiteral("delta"), piece}});
    }
    notify(QStringLiteral("item/completed"),
           {{QStringLiteral("threadId"), m_thread},
            {QStringLiteral("turnId"), m_turn},
            {QStringLiteral("item"), QJsonObject{{QStringLiteral("type"), QStringLiteral("agentMessage")},
                                                 {QStringLiteral("id"), item},
                                                 {QStringLiteral("text"), text}}}});
  }

  QJsonObject call(const QString& name, const QJsonObject& arguments, bool* error) {
    ++m_items;
    QJsonObject item{{QStringLiteral("type"), QStringLiteral("mcpToolCall")},
                     {QStringLiteral("id"), QStringLiteral("call_%1").arg(m_items)},
                     {QStringLiteral("server"), QStringLiteral("mitcad")},
                     {QStringLiteral("tool"), name},
                     {QStringLiteral("status"), QStringLiteral("inProgress")},
                     {QStringLiteral("arguments"), arguments}};
    notify(QStringLiteral("item/started"), {{QStringLiteral("threadId"), m_thread},
                                            {QStringLiteral("turnId"), m_turn},
                                            {QStringLiteral("item"), item}});
    const QJsonObject result = m_bridge.tool(name, arguments, error);
    QJsonArray content;
    content.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("text")},
                               {QStringLiteral("text"), QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact))}});
    item.insert(QStringLiteral("status"), *error ? QStringLiteral("failed") : QStringLiteral("completed"));
    if (*error) {
      item.insert(QStringLiteral("error"),
                  QJsonObject{{QStringLiteral("message"), result.value(QStringLiteral("error")).toString(QStringLiteral("refused"))}});
    } else {
      item.insert(QStringLiteral("result"),
                  QJsonObject{{QStringLiteral("content"), content}, {QStringLiteral("structuredContent"), result}});
    }
    notify(QStringLiteral("item/completed"), {{QStringLiteral("threadId"), m_thread},
                                              {QStringLiteral("turnId"), m_turn},
                                              {QStringLiteral("item"), item}});
    return result;
  }

  void end(const QString& status, const QString& error) {
    QJsonObject turn{{QStringLiteral("id"), m_turn}, {QStringLiteral("status"), status}};
    if (!error.isEmpty()) {
      turn.insert(QStringLiteral("error"), QJsonObject{{QStringLiteral("message"), error}});
    }
    notify(QStringLiteral("turn/completed"), {{QStringLiteral("threadId"), m_thread}, {QStringLiteral("turn"), turn}});
  }

  QString document() {
    bool error = false;
    const QJsonObject listed = m_bridge.tool(QStringLiteral("document_list"), QJsonObject(), &error);
    return listed.value(QStringLiteral("documents")).toArray().at(0).toObject().value(QStringLiteral("document")).toString();
  }

private:
  QString m_thread;
  QString m_turn;
  Bridge& m_bridge;
  int m_items = 0;
};

// The thread's configuration as MitCad must give it: its bridge as the only
// MCP server (with the chat's marker), Codex's own tools off, nothing asked.
bool expectedThread(const QJsonObject& params, QString* program, QStringList* arguments) {
  const QJsonObject config = params.value(QStringLiteral("config")).toObject();
  const QJsonObject servers = config.value(QStringLiteral("mcp_servers")).toObject();
  const QJsonObject server = servers.value(QStringLiteral("mitcad")).toObject();
  for (const QJsonValue& argument : server.value(QStringLiteral("args")).toArray()) {
    arguments->append(argument.toString());
  }
  *program = server.value(QStringLiteral("command")).toString();
  return servers.size() == 1 && !program->isEmpty() && arguments->contains(QStringLiteral("--chat-session")) &&
         params.value(QStringLiteral("approvalPolicy")).toString() == QStringLiteral("never") &&
         config.value(QStringLiteral("features.shell_tool")) == QJsonValue(false) &&
         config.value(QStringLiteral("sandbox_mode")).toString() == QStringLiteral("read-only");
}

int appServer(const QStringList& args) {
  appendLine(QStringLiteral("chat-starts.jsonl"),
             {{QStringLiteral("agent"), QStringLiteral("codex")}, {QStringLiteral("argv"), QJsonArray::fromStringList(args)}});
  const QDir threads(QDir(work()).filePath(QStringLiteral("threads")));
  QDir().mkpath(threads.path());
  Inbox inbox;
  Bridge bridge;
  bool connected = false;
  bool bridgeStarted = false;
  QString thread;
  QString model;
  int turns = 0;
  QJsonObject message;
  while (inbox.next(&message)) {
    const QString method = message.value(QStringLiteral("method")).toString();
    const QJsonValue id = message.value(QStringLiteral("id"));
    const QJsonObject params = message.value(QStringLiteral("params")).toObject();
    if (method == QStringLiteral("initialize")) {
      reply(id, {{QStringLiteral("userAgent"), QStringLiteral("codex-test")}});
    } else if (method == QStringLiteral("initialized")) {
      continue;
    } else if (method == QStringLiteral("model/list")) {
      QJsonArray data;
      data.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("test-codex-model")},
                              {QStringLiteral("model"), QStringLiteral("test-codex-model")},
                              {QStringLiteral("displayName"), QStringLiteral("Test model")},
                              {QStringLiteral("isDefault"), true}});
      data.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("test-codex-large")},
                              {QStringLiteral("model"), QStringLiteral("test-codex-large")},
                              {QStringLiteral("displayName"), QStringLiteral("Test large")}});
      reply(id, {{QStringLiteral("data"), data}});
    } else if (method == QStringLiteral("thread/start") || method == QStringLiteral("thread/resume")) {
      appendLine(QStringLiteral("chat-starts.jsonl"), {{QStringLiteral("agent"), QStringLiteral("codex")},
                                                       {QStringLiteral("method"), method},
                                                       {QStringLiteral("params"), params}});
      QString program;
      QStringList arguments;
      if (!expectedThread(params, &program, &arguments)) {
        std::fputs("test agent: unexpected thread configuration\n", stderr);
        replyError(id, QStringLiteral("test agent: unexpected thread configuration"));
        continue;
      }
      if (method == QStringLiteral("thread/resume")) {
        const QString wanted = params.value(QStringLiteral("threadId")).toString();
        if (wanted.isEmpty() || !QFileInfo::exists(threads.filePath(wanted))) {
          replyError(id, QStringLiteral("no rollout found for thread id %1").arg(wanted));
          continue;
        }
        thread = wanted;
      } else {
        thread = QStringLiteral("thr_") + QUuid::createUuid().toString(QUuid::Id128);
      }
      if (!bridgeStarted) {
        bridgeStarted = true;
        connected = bridge.start(program, arguments, QStringLiteral("codex-mcp-client"));
      }
      model = params.value(QStringLiteral("model")).toString(QStringLiteral("test-codex-model"));
      reply(id, {{QStringLiteral("thread"), QJsonObject{{QStringLiteral("id"), thread}}}, {QStringLiteral("model"), model}});
      notify(QStringLiteral("mcpServer/startupStatus/updated"),
             {{QStringLiteral("name"), QStringLiteral("mitcad")},
              {QStringLiteral("status"), connected ? QStringLiteral("ready") : QStringLiteral("failed")}});
    } else if (method == QStringLiteral("turn/start")) {
      ++turns;
      const QString turnId = QStringLiteral("turn_%1").arg(turns);
      reply(id, {{QStringLiteral("turn"), QJsonObject{{QStringLiteral("id"), turnId}, {QStringLiteral("status"), QStringLiteral("inProgress")}}}});
      notify(QStringLiteral("turn/started"), {{QStringLiteral("threadId"), thread},
                                              {QStringLiteral("turn"), QJsonObject{{QStringLiteral("id"), turnId}}}});
      QFile known(threads.filePath(thread));
      if (!known.exists() && known.open(QIODevice::WriteOnly)) {
        known.close();
      }
      QString text;
      QString context;
      QJsonArray pictures;
      for (const QJsonValue& value : params.value(QStringLiteral("input")).toArray()) {
        const QJsonObject part = value.toObject();
        const QString type = part.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("text")) {
          (text.isEmpty() ? text : context) = part.value(QStringLiteral("text")).toString();
        } else if (type == QStringLiteral("image")) {
          // data:image/png;base64,... : its media type.
          const QString url = part.value(QStringLiteral("url")).toString();
          pictures.append(url.mid(5, url.indexOf(QLatin1Char(';')) - 5));
        }
      }
      appendLine(QStringLiteral("chat-messages.jsonl"), {{QStringLiteral("agent"), QStringLiteral("codex")},
                                                         {QStringLiteral("session"), thread},
                                                         {QStringLiteral("text"), text},
                                                         {QStringLiteral("context"), context},
                                                         {QStringLiteral("pictures"), pictures},
                                                         {QStringLiteral("model"), params.value(QStringLiteral("model"))}});
      CodexTurn turn(thread, turnId, bridge);
      if (!connected) {
        turn.answer(QStringLiteral("MitCad's tools are not connected."));
        turn.end(QStringLiteral("failed"), QStringLiteral("MitCad's tools are not connected."));
        continue;
      }
      bool error = false;
      if (text.contains(QStringLiteral("box"))) {
        turn.answer(QStringLiteral("Adding the boxes."));
        const QString document = turn.document();
        turn.call(QStringLiteral("model_command"), box(document, 0, 20, 30, 10), &error);
        turn.call(QStringLiteral("model_command"), box(document, 40, 10, 10, 10), &error);
        turn.answer(QStringLiteral("Two boxes are in the design."));
        turn.end(QStringLiteral("completed"), QString());
      } else if (text.contains(QStringLiteral("save"))) {
        turn.answer(QStringLiteral("Saving the design."));
        const QString document = turn.document();
        turn.call(QStringLiteral("document_save"),
                  {{QStringLiteral("document"), document},
                   {QStringLiteral("path"), qEnvironmentVariable("MITCAD_TEST_CHAT_SAVE_PATH")},
                   {QStringLiteral("overwrite"), true}},
                  &error);
        turn.answer(error ? QStringLiteral("The design was not saved.") : QStringLiteral("Saved."));
        turn.end(QStringLiteral("completed"), QString());
      } else if (text.contains(QStringLiteral("wait"))) {
        turn.answer(QStringLiteral("Waiting to be stopped."));
        QJsonValue interrupt;
        bool asked = false;
        for (int waited = 0; waited < 1200 && !asked; ++waited) {
          asked = inbox.takeInterrupt(50, &interrupt);
        }
        if (asked) {
          reply(interrupt, QJsonObject());
        }
        turn.end(QStringLiteral("interrupted"), QString());
      } else {
        const QString answer = QStringLiteral("Hello from the test agent: %1 characters of context, %2 picture(s).")
                                   .arg(context.size())
                                   .arg(pictures.size());
        turn.answer(answer);
        turn.end(QStringLiteral("completed"), QString());
      }
    } else if (method == QStringLiteral("turn/interrupt")) {
      reply(id, QJsonObject());
    } else if (!id.isUndefined()) {
      replyError(id, QStringLiteral("test agent: %1 is not handled").arg(method));
    }
  }
  return 0;
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  const QStringList args = app.arguments().mid(1);
  // Copied as `codex` (codex.exe): Codex's commands.
  if (QFileInfo(QCoreApplication::applicationFilePath()).baseName() == QStringLiteral("codex")) {
    const QString codexIn = QDir(work()).filePath(QStringLiteral("codex-signed-in"));
    if (args == QStringList{QStringLiteral("--version")}) {
      std::fputs(qPrintable(QStringLiteral("codex-cli ") +
                            qEnvironmentVariable("MITCAD_TEST_CODEX_VERSION", QStringLiteral("0.95.0")) +
                            QStringLiteral("\n")),
                 stdout);
      return 0;
    }
    if (args == QStringList{QStringLiteral("login"), QStringLiteral("status")}) {
      const bool in = QFileInfo::exists(codexIn);
      std::fputs(in ? "Logged in using ChatGPT\n" : "Not logged in\n", stderr);
      return in ? 0 : 1;
    }
    if (args == QStringList{QStringLiteral("login")}) {
      std::fputs("If your browser did not open, navigate to this URL to authenticate:\n\n"
                 "https://auth.openai.com/oauth/authorize?test\n",
                 stdout);
      std::fflush(stdout);
      QFile file(codexIn);
      return file.open(QIODevice::WriteOnly) ? 0 : 1;
    }
    if (args == QStringList{QStringLiteral("app-server")}) {
      return appServer(args);
    }
    std::fputs("test agent: unexpected arguments for codex\n", stderr);
    return 2;
  }
  const QString signedIn = QDir(work()).filePath(QStringLiteral("claude-signed-in"));
  if (args == QStringList{QStringLiteral("--version")}) {
    std::fputs(qPrintable(qEnvironmentVariable("MITCAD_TEST_CHAT_VERSION", QStringLiteral("2.5.0")) +
                          QStringLiteral(" (Claude Code)\n")),
               stdout);
    return 0;
  }
  if (args.size() == 2 && args.first() == QStringLiteral("auth")) {
    if (args.at(1) == QStringLiteral("status")) {
      const bool in = QFileInfo::exists(signedIn);
      std::fputs(in ? "{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}\n" : "{\"loggedIn\": false}\n", stdout);
      return in ? 0 : 1;
    }
    if (args.at(1) == QStringLiteral("login")) {
      std::fputs("Opening https://claude.ai/oauth/authorize?test in your browser\n", stdout);
      std::fflush(stdout);
      QFile file(signedIn);
      return file.open(QIODevice::WriteOnly) ? 0 : 1;
    }
  }
  if (args.contains(QStringLiteral("-p"))) {
#ifndef _WIN32
    std::signal(SIGINT, onInterrupt);
#endif
    return printMode(args);
  }
  std::fputs("test agent: unexpected arguments\n", stderr);
  return 2;
}
