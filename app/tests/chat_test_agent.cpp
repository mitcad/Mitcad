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
//   "several"  three calls in a row (the second fails), a text, two calls
//              and a text: two group rows of the panel (mitcad#983)
//   "box"      two boxes (30 x 20 x 10 mm and 10 mm), two calls; with
//              "slowly" it waits between them for continue.png in the work
//              folder (the UI script's screenshot)
//   "save"     document_save to MITCAD_TEST_CHAT_SAVE_PATH
//   "new design" reads the open design, makes "nipple" with document_new,
//              builds a 10 mm box in it without a handle and reads both
//              designs again; chat-new-design.jsonl records it (mitcad#1266)
//   "undo"     model_command undo (a step from before the message)
//   "wait"     streams a line, then waits for its turn to be stopped
//   "account failure" (or the work folder's account-blocked file) an
//              account error; with "slowly" it waits for account-go.png;
//              the account check's probe fails for the network instead
//              while network-down.png is there and network-up.png not
//   otherwise  a plain answer naming what came with the message
//
// It records each start's arguments (chat-starts.jsonl) and each message's
// text, context and pictures (chat-messages.jsonl) in the work folder; a
// session it was given once (--session-id) can be resumed (--resume), an
// unknown one cannot. Never installed or linked into the application.
//
// Copied as `codex` it stands in for Codex the same way: `--version`,
// `login status` and `login` (the file codex-signed-in), and `app-server`,
// which speaks Codex's app-server JSON-RPC (core/mcp/src/chat/codex.rs) in
// the shapes of the JSON Schema `codex app-server generate-json-schema`
// writes (codex-cli 0.157.1): initialize and the initialized notification
// (without params), model/list (two test models), thread/start and
// thread/resume (it checks that MitCad's bridge is the only MCP server and
// that Codex's own tools are off with the keys the current Codex knows, and
// starts that bridge), turn/start (the same words as above, as agentMessage
// deltas, mcpToolCall items and turn/completed) and turn/interrupt ("wait"
// ends as interrupted). Its starts record the thread requests' parameters
// (the model among them), its messages the turn's model too.
//
// Copied as `acp-test-agent` it is a custom command that speaks the Agent
// Client Protocol (mitcad#1198, core/mcp/src/chat/acp.rs) in the shapes of
// the protocol's published schema (version 1): initialize (it checks that
// MitCad offers no file system and no terminal), session/new and
// session/load (MitCad's bridge as the only MCP server, which it starts; a
// loaded session replays an earlier exchange, which MitCad must not show
// again), a model selector (two test models, session/set_config_option),
// session/prompt by the same words as above (agent_message_chunk,
// tool_call and tool_call_update updates; "box" first asks permission for
// an edit of its own, which MitCad must reject, and reads a file through
// MitCad, which MitCad must refuse, then asks permission for MitCad's tool,
// which MitCad must allow) and session/cancel ("wait" ends cancelled). The
// permissions and the refusal are recorded in acp-checks.jsonl. Copied as
// `not-acp` it writes a line that is no message of the protocol and ends:
// a command that does not answer the handshake.
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <csignal>
#include <cstdio>
#include <deque>
#include <functional>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <utility>

#include <QCoreApplication>
#include <QDateTime>
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

QString accountFailure() {
  return QStringLiteral("Your organization has disabled Claude subscription access for Claude Code · "
                        "Use an Anthropic API key instead, or ask your admin to enable access");
}

// The account check's probe turn fails for the network (mitcad#1155): while
// the UI script's screenshot network-down.png is in the work folder and
// network-up.png is not.
bool networkDown(const QString& text) {
  const QDir folder(work());
  return text.startsWith(QStringLiteral("Reply with OK only.")) &&
         QFileInfo::exists(folder.filePath(QStringLiteral("network-down.png"))) &&
         !QFileInfo::exists(folder.filePath(QStringLiteral("network-up.png")));
}

QString networkFailure() { return QStringLiteral("API Error: Connection error (the test network is down)"); }

// "account failure, slowly": the failure waits for the UI script's
// screenshot account-go.png, so that a message can be queued meanwhile.
void waitForAccountGo(const QString& text) {
  if (!text.contains(QStringLiteral("slowly"))) {
    return;
  }
  const QString signal = QDir(work()).filePath(QStringLiteral("account-go.png"));
  for (int waited = 0; waited < 600 && !QFileInfo::exists(signal); ++waited) {
    std::this_thread::sleep_for(std::chrono::milliseconds(50));
  }
}

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

// model_query's arguments: one query of a document.
QJsonObject query(const QString& document, const QString& name) {
  QJsonObject inner;
  inner.insert(QStringLiteral("query"), name);
  QJsonObject arguments;
  arguments.insert(QStringLiteral("document"), document);
  arguments.insert(QStringLiteral("query"), inner);
  return arguments;
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
    if (networkDown(text)) {
      conversation.end(true, QStringLiteral("error_during_execution"), networkFailure());
    } else if (text.contains(QStringLiteral("account failure")) ||
        QFileInfo::exists(QDir(work()).filePath(QStringLiteral("account-blocked")))) {
      waitForAccountGo(text);
      // Stream it first, then repeat it in an assistant error and the
      // result: all three deliveries must become one error row.
      conversation.answer(accountFailure());
      const QJsonArray content{QJsonObject{{QStringLiteral("type"), QStringLiteral("text")},
                                           {QStringLiteral("text"), accountFailure()}}};
      emitLine({{QStringLiteral("type"), QStringLiteral("assistant")},
                {QStringLiteral("error"), QStringLiteral("authentication_failed")},
                {QStringLiteral("message"), QJsonObject{{QStringLiteral("content"), content}}}});
      conversation.end(true, QStringLiteral("error_during_execution"), accountFailure());
    } else if (text.contains(QStringLiteral("new design"))) {
      // A design from a design (mitcad#1266): read the open one, make a
      // new one beside it, build in it without a handle (the call goes to
      // the design made), and read both again.
      conversation.answer(QStringLiteral("Reading the open design."));
      const QString own = conversation.document();
      const QJsonObject read = conversation.call(QStringLiteral("model_query"), query(own, QStringLiteral("bodies")), &error);
      const qsizetype ownBodies = read.value(QStringLiteral("result")).toArray().size();
      QJsonObject named;
      named.insert(QStringLiteral("name"), QStringLiteral("nipple"));
      const QJsonObject made = conversation.call(QStringLiteral("document_new"), named, &error);
      const bool madeOk = !error;
      const QString design = made.value(QStringLiteral("document")).toString();
      QJsonObject build = box(QString(), 0, 10, 10, 10);
      build.remove(QStringLiteral("document"));
      QJsonObject built;
      bool buildError = true;
      for (int attempt = 0; attempt < 100; ++attempt) {
        built = conversation.call(QStringLiteral("model_command"), build, &buildError);
        if (!buildError || !built.value(QStringLiteral("busy")).toBool()) {
          break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
      }
      const QJsonObject newBodies = conversation.call(QStringLiteral("model_query"), query(design, QStringLiteral("bodies")), &error);
      const QJsonObject ownAfter = conversation.call(QStringLiteral("model_query"), query(own, QStringLiteral("bodies")), &error);
      const bool ok = madeOk && !design.isEmpty() && design != own &&
                      made.value(QStringLiteral("previous")).toString() == own && !buildError &&
                      built.value(QStringLiteral("document")).toString() == design &&
                      newBodies.value(QStringLiteral("result")).toArray().size() == 1 &&
                      ownAfter.value(QStringLiteral("result")).toArray().size() == ownBodies;
      appendLine(QStringLiteral("chat-new-design.jsonl"),
                 {{QStringLiteral("own"), own},
                  {QStringLiteral("design"), design},
                  {QStringLiteral("path"), made.value(QStringLiteral("path"))},
                  {QStringLiteral("built_in"), built.value(QStringLiteral("document"))},
                  {QStringLiteral("own_bodies"), static_cast<qint64>(ownBodies)},
                  {QStringLiteral("own_bodies_after"), static_cast<qint64>(ownAfter.value(QStringLiteral("result")).toArray().size())},
                  {QStringLiteral("new_bodies"), static_cast<qint64>(newBodies.value(QStringLiteral("result")).toArray().size())},
                  {QStringLiteral("ok"), ok}});
      const QString reply = ok ? QStringLiteral("The new design has its first body; the open design is unchanged.")
                               : QStringLiteral("The new design could not be built.");
      conversation.answer(reply);
      conversation.end(!ok, ok ? QStringLiteral("success") : QStringLiteral("error_during_execution"), reply);
    } else if (text.contains(QStringLiteral("several"))) {
      // Three calls in a row (the second fails: no such document), a text,
      // two calls, a text: two group rows (mitcad#983).
      conversation.answer(QStringLiteral("Looking at the design."));
      const QString document = conversation.document();
      conversation.call(QStringLiteral("model_query"), query(document, QStringLiteral("bodies")), &error);
      conversation.call(QStringLiteral("model_query"), query(QStringLiteral("no-such-document"), QStringLiteral("bodies")),
                        &error);
      conversation.call(QStringLiteral("document_list"), QJsonObject(), &error);
      conversation.answer(QStringLiteral("Two more calls."));
      conversation.call(QStringLiteral("model_query"), query(document, QStringLiteral("timeline")), &error);
      conversation.call(QStringLiteral("ui_state"), QJsonObject(), &error);
      conversation.answer(QStringLiteral("Five calls made."));
      conversation.end(false, QStringLiteral("success"), QStringLiteral("Five calls made."));
    } else if (text.contains(QStringLiteral("box"))) {
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

  // The first waiting message that `wanted` accepts, taken out; waits up
  // to `ms` for it.
  bool take(const std::function<bool(const QJsonObject&)>& wanted, int ms, QJsonObject* found) {
    std::unique_lock<std::mutex> lock(m_mutex);
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(ms);
    for (;;) {
      for (auto line = m_lines.begin(); line != m_lines.end(); ++line) {
        const QJsonObject message = QJsonDocument::fromJson(QByteArray::fromStdString(*line)).object();
        if (wanted(message)) {
          *found = message;
          m_lines.erase(line);
          return true;
        }
      }
      if (m_closed || std::chrono::steady_clock::now() >= deadline) {
        return false;
      }
      m_ready.wait_until(lock, deadline);
    }
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

qint64 nowMs() { return QDateTime::currentMSecsSinceEpoch(); }

// A turn as the protocol's Turn: its items are not listed here.
QJsonObject turnObject(const QString& id, const QString& status) {
  return {{QStringLiteral("id"), id}, {QStringLiteral("items"), QJsonArray()}, {QStringLiteral("status"), status}};
}

// thread/start's and thread/resume's result, with the fields the schema
// requires.
QJsonObject threadResult(const QString& thread, const QString& model, const QString& cwd) {
  const qint64 now = nowMs() / 1000;
  const QJsonObject object{{QStringLiteral("id"), thread},
                           {QStringLiteral("sessionId"), thread},
                           {QStringLiteral("cliVersion"), QStringLiteral("0.157.1")},
                           {QStringLiteral("createdAt"), now},
                           {QStringLiteral("updatedAt"), now},
                           {QStringLiteral("cwd"), cwd},
                           {QStringLiteral("ephemeral"), false},
                           {QStringLiteral("modelProvider"), QStringLiteral("openai")},
                           {QStringLiteral("preview"), QString()},
                           {QStringLiteral("projectId"), QJsonValue()},
                           {QStringLiteral("source"), QStringLiteral("appServer")},
                           {QStringLiteral("status"), QJsonObject{{QStringLiteral("type"), QStringLiteral("idle")}}},
                           {QStringLiteral("turns"), QJsonArray()}};
  return {{QStringLiteral("thread"), object},
          {QStringLiteral("model"), model},
          {QStringLiteral("modelProvider"), QStringLiteral("openai")},
          {QStringLiteral("cwd"), cwd},
          {QStringLiteral("approvalPolicy"), QStringLiteral("never")},
          {QStringLiteral("approvalsReviewer"), QStringLiteral("user")},
          {QStringLiteral("sandbox"), QJsonObject{{QStringLiteral("type"), QStringLiteral("readOnly")}}}};
}

QJsonObject testModel(const QString& id, const QString& name, bool isDefault) {
  return {{QStringLiteral("id"), id},
          {QStringLiteral("model"), id},
          {QStringLiteral("displayName"), name},
          {QStringLiteral("description"), QStringLiteral("A model of the test agent")},
          {QStringLiteral("hidden"), false},
          {QStringLiteral("isDefault"), isDefault},
          {QStringLiteral("defaultReasoningEffort"), QStringLiteral("medium")},
          {QStringLiteral("supportedReasoningEfforts"), QJsonArray()}};
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
            {QStringLiteral("completedAtMs"), nowMs()},
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
                                            {QStringLiteral("startedAtMs"), nowMs()},
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
                                              {QStringLiteral("completedAtMs"), nowMs()},
                                              {QStringLiteral("item"), item}});
    return result;
  }

  void end(const QString& status, const QString& error) {
    QJsonObject turn = turnObject(m_turn, status);
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
// MCP server (with the chat's marker), Codex's own tools off with the keys
// the current Codex knows (none it dropped), nothing asked.
bool expectedThread(const QJsonObject& params, QString* program, QStringList* arguments) {
  const QJsonObject config = params.value(QStringLiteral("config")).toObject();
  const QJsonObject servers = config.value(QStringLiteral("mcp_servers")).toObject();
  const QJsonObject server = servers.value(QStringLiteral("mitcad")).toObject();
  for (const QJsonValue& argument : server.value(QStringLiteral("args")).toArray()) {
    arguments->append(argument.toString());
  }
  *program = server.value(QStringLiteral("command")).toString();
  for (const char* off : {"features.shell_tool", "features.unified_exec", "features.view_image"}) {
    if (config.value(QString::fromLatin1(off)) != QJsonValue(false)) {
      return false;
    }
  }
  for (const char* stale : {"include_apply_patch_tool", "tools.web_search", "tools.view_image"}) {
    if (config.contains(QString::fromLatin1(stale))) {
      return false;
    }
  }
  return servers.size() == 1 && !program->isEmpty() && arguments->contains(QStringLiteral("--chat-session")) &&
         params.value(QStringLiteral("approvalPolicy")).toString() == QStringLiteral("never") &&
         config.value(QStringLiteral("web_search")).toString() == QStringLiteral("disabled") &&
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
  bool initialized = false;
  QString thread;
  QString model;
  int turns = 0;
  QJsonObject message;
  while (inbox.next(&message)) {
    const QString method = message.value(QStringLiteral("method")).toString();
    const QJsonValue id = message.value(QStringLiteral("id"));
    const QJsonObject params = message.value(QStringLiteral("params")).toObject();
    if (method == QStringLiteral("initialize")) {
      reply(id, {{QStringLiteral("codexHome"), QDir(work()).absoluteFilePath(QStringLiteral("codex-home"))},
                 {QStringLiteral("platformFamily"), QStringLiteral("unix")},
                 {QStringLiteral("platformOs"), QStringLiteral("linux")},
                 {QStringLiteral("userAgent"), QStringLiteral("codex-test")}});
    } else if (method == QStringLiteral("initialized")) {
      // The schema's notification has no params.
      initialized = !message.contains(QStringLiteral("params"));
      if (!initialized) {
        std::fputs("test agent: initialized with params\n", stderr);
      }
    } else if (!initialized && !id.isUndefined()) {
      replyError(id, QStringLiteral("Not initialized"));
    } else if (method == QStringLiteral("model/list")) {
      QJsonArray data;
      data.append(testModel(QStringLiteral("test-codex-model"), QStringLiteral("Test model"), true));
      data.append(testModel(QStringLiteral("test-codex-large"), QStringLiteral("Test large"), false));
      reply(id, {{QStringLiteral("data"), data}, {QStringLiteral("nextCursor"), QJsonValue()}});
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
      reply(id, threadResult(thread, model, params.value(QStringLiteral("cwd")).toString(QDir::currentPath())));
      QJsonObject status{{QStringLiteral("name"), QStringLiteral("mitcad")},
                         {QStringLiteral("threadId"), thread},
                         {QStringLiteral("status"), connected ? QStringLiteral("ready") : QStringLiteral("failed")}};
      if (!connected) {
        status.insert(QStringLiteral("error"), QStringLiteral("test agent: the bridge did not start"));
      }
      notify(QStringLiteral("mcpServer/startupStatus/updated"), status);
    } else if (method == QStringLiteral("turn/start")) {
      ++turns;
      const QString turnId = QStringLiteral("turn_%1").arg(turns);
      const QJsonObject started = turnObject(turnId, QStringLiteral("inProgress"));
      reply(id, {{QStringLiteral("turn"), started}});
      notify(QStringLiteral("turn/started"), {{QStringLiteral("threadId"), thread}, {QStringLiteral("turn"), started}});
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
      if (networkDown(text)) {
        turn.end(QStringLiteral("failed"), networkFailure());
      } else if (text.contains(QStringLiteral("account failure")) ||
        QFileInfo::exists(QDir(work()).filePath(QStringLiteral("account-blocked")))) {
        waitForAccountGo(text);
        const QJsonObject connection{{QStringLiteral("httpStatusCode"), 401}};
        const QJsonObject info{{QStringLiteral("httpConnectionFailed"), connection}};
        const QJsonObject failure{{QStringLiteral("message"), QStringLiteral("Not signed in")},
                                  {QStringLiteral("codexErrorInfo"), info}};
        notify(QStringLiteral("error"), {{QStringLiteral("threadId"), thread},
                                         {QStringLiteral("turnId"), turnId},
                                         {QStringLiteral("willRetry"), false},
                                         {QStringLiteral("error"), failure}});
        turn.end(QStringLiteral("failed"), QStringLiteral("Not signed in"));
      } else if (text.contains(QStringLiteral("box"))) {
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

// The Agent Client Protocol (mitcad#1198): JSON-RPC 2.0, one message per
// line.
void acpEmit(QJsonObject message) {
  message.insert(QStringLiteral("jsonrpc"), QStringLiteral("2.0"));
  emitLine(message);
}

void acpReply(const QJsonValue& id, const QJsonObject& result) {
  acpEmit({{QStringLiteral("id"), id}, {QStringLiteral("result"), result}});
}

void acpError(const QJsonValue& id, int code, const QString& message) {
  acpEmit({{QStringLiteral("id"), id},
           {QStringLiteral("error"), QJsonObject{{QStringLiteral("code"), code}, {QStringLiteral("message"), message}}}});
}

void acpUpdate(const QString& session, const QJsonObject& update) {
  acpEmit({{QStringLiteral("method"), QStringLiteral("session/update")},
           {QStringLiteral("params"), QJsonObject{{QStringLiteral("sessionId"), session}, {QStringLiteral("update"), update}}}});
}

QJsonObject acpText(const QString& text) {
  return {{QStringLiteral("type"), QStringLiteral("text")}, {QStringLiteral("text"), text}};
}

// The session's model selector, its two test models.
QJsonArray acpConfig(const QString& current) {
  QJsonArray options;
  options.append(QJsonObject{{QStringLiteral("value"), QStringLiteral("test-acp-model")},
                             {QStringLiteral("name"), QStringLiteral("Test ACP model")}});
  options.append(QJsonObject{{QStringLiteral("value"), QStringLiteral("test-acp-large")},
                             {QStringLiteral("name"), QStringLiteral("Test ACP large")}});
  QJsonArray config;
  config.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("model")},
                            {QStringLiteral("name"), QStringLiteral("Model")},
                            {QStringLiteral("category"), QStringLiteral("model")},
                            {QStringLiteral("type"), QStringLiteral("select")},
                            {QStringLiteral("currentValue"), current},
                            {QStringLiteral("options"), options}});
  return config;
}

class AcpTurn {
public:
  AcpTurn(QString session, Bridge& bridge, Inbox& inbox, int& next)
      : m_session(std::move(session)), m_bridge(bridge), m_inbox(inbox), m_next(next) {}

  void answer(const QString& text) {
    const qsizetype half = text.size() / 2;
    const QStringList pieces{text.left(half), text.mid(half)};
    for (const QString& piece : pieces) {
      acpUpdate(m_session, {{QStringLiteral("sessionUpdate"), QStringLiteral("agent_message_chunk")},
                            {QStringLiteral("content"), acpText(piece)}});
    }
  }

  // A request of the agent to MitCad; its answer (empty: none in time).
  QJsonObject ask(const QString& method, const QJsonObject& params) {
    const int id = m_next++;
    acpEmit({{QStringLiteral("id"), id}, {QStringLiteral("method"), method}, {QStringLiteral("params"), params}});
    QJsonObject answer;
    m_inbox.take([id](const QJsonObject& message) {
      return !message.contains(QStringLiteral("method")) && message.value(QStringLiteral("id")).toInt(-1) == id;
    }, 60000, &answer);
    return answer;
  }

  // Permission for a tool call, with the four options; the option chosen
  // (empty: refused or cancelled), recorded in acp-checks.jsonl.
  QString permission(const QString& call, const QString& title, const QString& kind) {
    QJsonArray options;
    for (const char* option : {"allow_once", "allow_always", "reject_once", "reject_always"}) {
      options.append(QJsonObject{{QStringLiteral("optionId"), QStringLiteral("opt_") + QString::fromLatin1(option)},
                                 {QStringLiteral("name"), QString::fromLatin1(option)},
                                 {QStringLiteral("kind"), QString::fromLatin1(option)}});
    }
    const QJsonObject answer = ask(
        QStringLiteral("session/request_permission"),
        {{QStringLiteral("sessionId"), m_session},
         {QStringLiteral("toolCall"), QJsonObject{{QStringLiteral("toolCallId"), call},
                                                  {QStringLiteral("title"), title},
                                                  {QStringLiteral("kind"), kind}}},
         {QStringLiteral("options"), options}});
    const QJsonObject outcome = answer.value(QStringLiteral("result")).toObject().value(QStringLiteral("outcome")).toObject();
    const QString chosen = outcome.value(QStringLiteral("optionId")).toString();
    appendLine(QStringLiteral("acp-checks.jsonl"), {{QStringLiteral("check"), QStringLiteral("permission")},
                                                    {QStringLiteral("title"), title},
                                                    {QStringLiteral("outcome"), outcome.value(QStringLiteral("outcome"))},
                                                    {QStringLiteral("option"), chosen}});
    return chosen;
  }

  // A file read through MitCad: it must be refused (no file system).
  void readFile() {
    const QJsonObject answer = ask(QStringLiteral("fs/read_text_file"),
                                   {{QStringLiteral("sessionId"), m_session},
                                    {QStringLiteral("path"), QDir(work()).absoluteFilePath(QStringLiteral("notes.txt"))}});
    appendLine(QStringLiteral("acp-checks.jsonl"),
               {{QStringLiteral("check"), QStringLiteral("fs/read_text_file")},
                {QStringLiteral("error"), answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("code"))}});
  }

  // One of MitCad's tools through the bridge, shown as the protocol's tool
  // call; `asked`: permission is asked first.
  QJsonObject call(const QString& name, const QJsonObject& arguments, bool* error, bool asked = false) {
    ++m_calls;
    const QString id = QStringLiteral("call_%1").arg(m_calls);
    const QString title = QStringLiteral("mcp__mitcad__") + name;
    acpUpdate(m_session, {{QStringLiteral("sessionUpdate"), QStringLiteral("tool_call")},
                          {QStringLiteral("toolCallId"), id},
                          {QStringLiteral("title"), title},
                          {QStringLiteral("kind"), QStringLiteral("other")},
                          {QStringLiteral("status"), QStringLiteral("pending")},
                          {QStringLiteral("rawInput"), arguments}});
    if (asked && !permission(id, title, QStringLiteral("other")).startsWith(QStringLiteral("opt_allow"))) {
      *error = true;
      acpUpdate(m_session, {{QStringLiteral("sessionUpdate"), QStringLiteral("tool_call_update")},
                            {QStringLiteral("toolCallId"), id},
                            {QStringLiteral("status"), QStringLiteral("failed")}});
      return {};
    }
    acpUpdate(m_session, {{QStringLiteral("sessionUpdate"), QStringLiteral("tool_call_update")},
                          {QStringLiteral("toolCallId"), id},
                          {QStringLiteral("status"), QStringLiteral("in_progress")}});
    const QJsonObject result = m_bridge.tool(name, arguments, error);
    QJsonArray content;
    content.append(QJsonObject{{QStringLiteral("type"), QStringLiteral("content")},
                               {QStringLiteral("content"), acpText(QString::fromUtf8(QJsonDocument(result).toJson(QJsonDocument::Compact)))}});
    acpUpdate(m_session, {{QStringLiteral("sessionUpdate"), QStringLiteral("tool_call_update")},
                          {QStringLiteral("toolCallId"), id},
                          {QStringLiteral("status"), *error ? QStringLiteral("failed") : QStringLiteral("completed")},
                          {QStringLiteral("content"), content}});
    return result;
  }

  QString document() {
    bool error = false;
    const QJsonObject listed = m_bridge.tool(QStringLiteral("document_list"), QJsonObject(), &error);
    return listed.value(QStringLiteral("documents")).toArray().at(0).toObject().value(QStringLiteral("document")).toString();
  }

private:
  QString m_session;
  Bridge& m_bridge;
  Inbox& m_inbox;
  int& m_next;
  int m_calls = 0;
};

// The session's MCP servers as MitCad must give them: its bridge alone, with
// the chat's marker.
bool expectedServers(const QJsonObject& params, QString* program, QStringList* arguments) {
  const QJsonArray servers = params.value(QStringLiteral("mcpServers")).toArray();
  if (servers.size() != 1) {
    return false;
  }
  const QJsonObject server = servers.at(0).toObject();
  for (const QJsonValue& argument : server.value(QStringLiteral("args")).toArray()) {
    arguments->append(argument.toString());
  }
  *program = server.value(QStringLiteral("command")).toString();
  return server.value(QStringLiteral("name")).toString() == QStringLiteral("mitcad") && !program->isEmpty() &&
         server.value(QStringLiteral("env")).isArray() && arguments->contains(QStringLiteral("--chat-session")) &&
         QDir::isAbsolutePath(params.value(QStringLiteral("cwd")).toString());
}

int acpAgent(const QStringList& args) {
  appendLine(QStringLiteral("chat-starts.jsonl"),
             {{QStringLiteral("agent"), QStringLiteral("acp")}, {QStringLiteral("argv"), QJsonArray::fromStringList(args)}});
  const QDir sessions(QDir(work()).filePath(QStringLiteral("acp-sessions")));
  QDir().mkpath(sessions.path());
  Inbox inbox;
  Bridge bridge;
  bool connected = false;
  bool bridgeStarted = false;
  QString session;
  QString model = QStringLiteral("test-acp-model");
  int next = 100;
  QJsonObject message;
  while (inbox.next(&message)) {
    const QString method = message.value(QStringLiteral("method")).toString();
    const QJsonValue id = message.value(QStringLiteral("id"));
    const QJsonObject params = message.value(QStringLiteral("params")).toObject();
    if (message.value(QStringLiteral("jsonrpc")).toString() != QStringLiteral("2.0")) {
      std::fputs("test agent: a message without jsonrpc 2.0\n", stderr);
      if (!id.isUndefined()) {
        acpError(id, -32600, QStringLiteral("Invalid Request"));
      }
      continue;
    }
    if (method == QStringLiteral("initialize")) {
      const QJsonObject client = params.value(QStringLiteral("clientCapabilities")).toObject();
      const QJsonObject files = client.value(QStringLiteral("fs")).toObject();
      if (params.value(QStringLiteral("protocolVersion")).toInt() != 1 ||
          files.value(QStringLiteral("readTextFile")).toBool(true) || files.value(QStringLiteral("writeTextFile")).toBool(true) ||
          client.value(QStringLiteral("terminal")).toBool(true)) {
        std::fputs("test agent: unexpected initialize\n", stderr);
        acpError(id, -32602, QStringLiteral("test agent: unexpected initialize"));
        continue;
      }
      acpReply(id, {{QStringLiteral("protocolVersion"), 1},
                    {QStringLiteral("agentCapabilities"),
                     QJsonObject{{QStringLiteral("loadSession"), true},
                                 {QStringLiteral("promptCapabilities"),
                                  QJsonObject{{QStringLiteral("image"), true},
                                              {QStringLiteral("audio"), false},
                                              {QStringLiteral("embeddedContext"), false}}}}},
                    {QStringLiteral("agentInfo"), QJsonObject{{QStringLiteral("name"), QStringLiteral("acp-test-agent")},
                                                              {QStringLiteral("title"), QStringLiteral("ACP test agent")},
                                                              {QStringLiteral("version"), QStringLiteral("1.0.0")}}},
                    {QStringLiteral("authMethods"), QJsonArray()}});
    } else if (method == QStringLiteral("session/new") || method == QStringLiteral("session/load")) {
      appendLine(QStringLiteral("chat-starts.jsonl"), {{QStringLiteral("agent"), QStringLiteral("acp")},
                                                       {QStringLiteral("method"), method},
                                                       {QStringLiteral("params"), params}});
      QString program;
      QStringList arguments;
      if (!expectedServers(params, &program, &arguments)) {
        std::fputs("test agent: unexpected MCP servers\n", stderr);
        acpError(id, -32602, QStringLiteral("test agent: unexpected MCP servers"));
        continue;
      }
      if (method == QStringLiteral("session/load")) {
        const QString wanted = params.value(QStringLiteral("sessionId")).toString();
        if (wanted.isEmpty() || !QFileInfo::exists(sessions.filePath(wanted))) {
          acpError(id, -32002, QStringLiteral("Resource not found: session %1").arg(wanted));
          continue;
        }
        session = wanted;
      } else {
        session = QStringLiteral("sess_") + QUuid::createUuid().toString(QUuid::Id128);
      }
      if (!bridgeStarted) {
        bridgeStarted = true;
        connected = bridge.start(program, arguments, QStringLiteral("acp-test-agent"));
      }
      if (method == QStringLiteral("session/load")) {
        // The protocol's replay of the conversation before the answer:
        // MitCad shows its own history instead.
        acpUpdate(session, {{QStringLiteral("sessionUpdate"), QStringLiteral("user_message_chunk")},
                            {QStringLiteral("content"), acpText(QStringLiteral("An earlier message, replayed."))}});
        acpUpdate(session, {{QStringLiteral("sessionUpdate"), QStringLiteral("agent_message_chunk")},
                            {QStringLiteral("content"), acpText(QStringLiteral("An earlier answer, replayed."))}});
        acpReply(id, {{QStringLiteral("configOptions"), acpConfig(model)}});
      } else {
        acpReply(id, {{QStringLiteral("sessionId"), session}, {QStringLiteral("configOptions"), acpConfig(model)}});
      }
    } else if (method == QStringLiteral("session/set_config_option")) {
      appendLine(QStringLiteral("chat-starts.jsonl"), {{QStringLiteral("agent"), QStringLiteral("acp")},
                                                       {QStringLiteral("method"), method},
                                                       {QStringLiteral("params"), params}});
      if (params.value(QStringLiteral("configId")).toString() != QStringLiteral("model")) {
        acpError(id, -32602, QStringLiteral("test agent: no such option"));
        continue;
      }
      model = params.value(QStringLiteral("value")).toString();
      acpReply(id, {{QStringLiteral("configOptions"), acpConfig(model)}});
    } else if (method == QStringLiteral("session/prompt")) {
      if (params.value(QStringLiteral("sessionId")).toString() != session || session.isEmpty()) {
        acpError(id, -32602, QStringLiteral("test agent: no such session"));
        continue;
      }
      QFile known(sessions.filePath(session));
      if (!known.exists() && known.open(QIODevice::WriteOnly)) {
        known.close();
      }
      QString text;
      QString context;
      QJsonArray pictures;
      for (const QJsonValue& value : params.value(QStringLiteral("prompt")).toArray()) {
        const QJsonObject part = value.toObject();
        const QString type = part.value(QStringLiteral("type")).toString();
        if (type == QStringLiteral("text")) {
          (text.isEmpty() ? text : context) = part.value(QStringLiteral("text")).toString();
        } else if (type == QStringLiteral("image")) {
          pictures.append(part.value(QStringLiteral("mimeType")));
        }
      }
      appendLine(QStringLiteral("chat-messages.jsonl"), {{QStringLiteral("agent"), QStringLiteral("acp")},
                                                         {QStringLiteral("session"), session},
                                                         {QStringLiteral("text"), text},
                                                         {QStringLiteral("context"), context},
                                                         {QStringLiteral("pictures"), pictures},
                                                         {QStringLiteral("model"), model}});
      AcpTurn turn(session, bridge, inbox, next);
      const auto stopped = [&id](const QString& reason) { acpReply(id, {{QStringLiteral("stopReason"), reason}}); };
      if (!connected) {
        turn.answer(QStringLiteral("MitCad's tools are not connected."));
        acpError(id, -32603, QStringLiteral("MitCad's tools are not connected."));
        continue;
      }
      bool error = false;
      if (networkDown(text)) {
        acpError(id, -32603, networkFailure());
      } else if (text.contains(QStringLiteral("box"))) {
        turn.answer(QStringLiteral("Adding the boxes."));
        // An edit of its own and a file read through MitCad first: both
        // must be refused.
        turn.permission(QStringLiteral("call_own"), QStringLiteral("Write notes.txt"), QStringLiteral("edit"));
        turn.readFile();
        const QString document = turn.document();
        turn.call(QStringLiteral("model_command"), box(document, 0, 20, 30, 10), &error, true);
        turn.call(QStringLiteral("model_command"), box(document, 40, 10, 10, 10), &error);
        turn.answer(QStringLiteral("Two boxes are in the design."));
        stopped(QStringLiteral("end_turn"));
      } else if (text.contains(QStringLiteral("save"))) {
        turn.answer(QStringLiteral("Saving the design."));
        const QString document = turn.document();
        turn.call(QStringLiteral("document_save"),
                  {{QStringLiteral("document"), document},
                   {QStringLiteral("path"), qEnvironmentVariable("MITCAD_TEST_CHAT_SAVE_PATH")},
                   {QStringLiteral("overwrite"), true}},
                  &error);
        turn.answer(error ? QStringLiteral("The design was not saved.") : QStringLiteral("Saved."));
        stopped(QStringLiteral("end_turn"));
      } else if (text.contains(QStringLiteral("wait"))) {
        turn.answer(QStringLiteral("Waiting to be stopped."));
        QJsonObject cancel;
        const bool asked = inbox.take([](const QJsonObject& candidate) {
          return candidate.value(QStringLiteral("method")).toString() == QStringLiteral("session/cancel");
        }, 60000, &cancel);
        stopped(asked ? QStringLiteral("cancelled") : QStringLiteral("end_turn"));
      } else {
        turn.answer(QStringLiteral("Hello from the test agent: %1 characters of context, %2 picture(s).")
                        .arg(context.size())
                        .arg(pictures.size()));
        stopped(QStringLiteral("end_turn"));
      }
    } else if (method == QStringLiteral("session/cancel")) {
      // Nothing runs: nothing to cancel.
    } else if (!id.isUndefined() && !method.isEmpty()) {
      acpError(id, -32601, QStringLiteral("test agent: %1 is not handled").arg(method));
    }
  }
  return 0;
}

} // namespace

int main(int argc, char** argv) {
  QCoreApplication app(argc, argv);
  const QStringList args = app.arguments().mid(1);
  const QString called = QFileInfo(QCoreApplication::applicationFilePath()).baseName();
  // Copied as `acp-test-agent`: a custom command over the Agent Client
  // Protocol; as `not-acp`: a command that does not answer its handshake.
  if (called == QStringLiteral("acp-test-agent")) {
    return acpAgent(args);
  }
  if (called == QStringLiteral("not-acp")) {
    // It answers nothing, and ends with its input.
    std::fputs("This program does not speak the Agent Client Protocol.\n", stdout);
    std::fflush(stdout);
    std::string line;
    while (std::getline(std::cin, line)) {
    }
    return 0;
  }
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
