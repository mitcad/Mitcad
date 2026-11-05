// SPDX-License-Identifier: MIT
#include "ChatController.hpp"

#include <algorithm>
#include <utility>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QTimer>
#include <QUuid>
#include <QtLogging>

#include "../mcp/AssistantSetup.hpp"
#include "AccountCheck.hpp"
#include "AcpAgent.hpp"
#include "ApiAgent.hpp"
#include "ChatLogic.hpp"
#include "ChatSettings.hpp"
#include "ClaudeCodeAgent.hpp"
#include "CodexAgent.hpp"

namespace mitcad::agent {
namespace {

// What an untitled design's history keeps in memory at most.
constexpr qsizetype kUnsavedLimit = 4 * 1024 * 1024;
// Check again's setup check at most (mitcad#1155): the agents' version and
// sign-in probes take up to 20 s each.
constexpr int kAccountSetupCheckMs = 45000;

QString newSession() { return QUuid::createUuid().toString(QUuid::WithoutBraces); }

QString readinessName(ChatAgent::Readiness readiness) {
  switch (readiness) {
  case ChatAgent::Readiness::Checking: return QStringLiteral("checking");
  case ChatAgent::Readiness::Missing: return QStringLiteral("missing");
  case ChatAgent::Readiness::Outdated: return QStringLiteral("outdated");
  case ChatAgent::Readiness::SignedOut: return QStringLiteral("signed out");
  case ChatAgent::Readiness::SigningIn: return QStringLiteral("signing in");
  case ChatAgent::Readiness::Ready: return QStringLiteral("ready");
  }
  return QString();
}

QString activityName(ChatController::Activity activity) {
  switch (activity) {
  case ChatController::Activity::Idle: return QStringLiteral("idle");
  case ChatController::Activity::Starting: return QStringLiteral("starting");
  case ChatController::Activity::Working: return QStringLiteral("working");
  case ChatController::Activity::Stopping: return QStringLiteral("stopping");
  }
  return QString();
}

QString compact(const QJsonValue& value, qsizetype limit) {
  const QString text = value.isObject()  ? QString::fromUtf8(QJsonDocument(value.toObject()).toJson(QJsonDocument::Compact))
                       : value.isArray() ? QString::fromUtf8(QJsonDocument(value.toArray()).toJson(QJsonDocument::Compact))
                                         : value.toVariant().toString();
  return text.size() > limit ? text.left(limit) + QStringLiteral("…") : text;
}

// A tool call in a few words for its row: the command, the query, the file.
QString toolSummary(const QString& name, const QJsonObject& input) {
  if (name == QStringLiteral("model_command")) {
    const QJsonValue command = input.value(QStringLiteral("command"));
    if (command.isArray()) {
      QStringList kinds;
      for (const QJsonValue& entry : command.toArray()) {
        kinds << entry.toObject().value(QStringLiteral("cmd")).toString();
      }
      return kinds.join(QStringLiteral(", "));
    }
    const QJsonObject single = command.toObject();
    const QString type = single.value(QStringLiteral("def")).toObject().value(QStringLiteral("type")).toString();
    const QString cmd = single.value(QStringLiteral("cmd")).toString();
    return type.isEmpty() ? cmd : cmd + QLatin1Char(' ') + type;
  }
  if (name == QStringLiteral("model_query")) {
    return input.value(QStringLiteral("query")).toObject().value(QStringLiteral("query")).toString();
  }
  if (name == QStringLiteral("ui_command")) {
    const QString id = input.value(QStringLiteral("id")).toString();
    return id.isEmpty() ? input.value(QStringLiteral("edit")).toString() : id;
  }
  const QString path = input.value(QStringLiteral("path")).toString();
  return path.isEmpty() ? QString() : QDir::toNativeSeparators(path);
}

} // namespace

ChatController::ChatController(ChatHost host, QObject* parent)
    : QObject(parent), m_host(std::move(host)), m_store(chatDataDirectory()) {
  // Each agent behind the same interface (mitcad#947), in the switcher's
  // order.
  m_agents.push_back(new ClaudeCodeAgent(this));
  m_agents.push_back(new CodexAgent(this));
  // The API endpoints and local models (phase 2): offered once enabled in
  // Preferences.
  for (const ApiAgent::Preset& preset : ApiAgent::presets()) {
    m_agents.push_back(new ApiAgent(preset, this));
  }
  // A custom command over the Agent Client Protocol (mitcad#1198): offered
  // once set in Preferences.
  m_agents.push_back(new AcpAgent(this));
  // Every other Claude Code found, beside the first (mitcad#1289), comes
  // with the first look.
  noteDetected();
  // The last used one; when it is not found and another is, that one.
  const QString chosen = ChatSettings::load().agent;
  m_agent = m_agents.front();
  for (ChatAgent* candidate : m_agents) {
    if (candidate->id() == chosen) {
      m_agent = candidate;
    }
  }
  if (!m_detected.contains(m_agent->id()) && !m_detected.isEmpty()) {
    for (ChatAgent* candidate : m_agents) {
      if (candidate->id() == m_detected.first()) {
        m_agent = candidate;
      }
    }
  }
  // The bridge the agents start (mitcad#1170: inside an AppImage too).
  const QString bridge = mcp::bundledBridge();
  qInfo().noquote() << (bridge.isEmpty() ? QStringLiteral("Chat: bridge not found")
                                         : QStringLiteral("Chat: bridge %1").arg(QDir::toNativeSeparators(bridge)));
  qInfo().noquote() << QStringLiteral("Chat: agent %1 (found: %2)")
                           .arg(choiceName(m_agent), m_detected.isEmpty() ? QStringLiteral("none")
                                                                          : m_detected.join(QStringLiteral(", ")));
  for (ChatAgent* candidate : m_agents) {
    adopt(candidate);
  }
  // Preferences cleared every design's history.
  connect(&ChatNotifier::instance(), &ChatNotifier::historyCleared, this, [this] {
    m_unsaved.clear();
    startOver(false);
  });
  // Preferences chose the agent.
  connect(&ChatNotifier::instance(), &ChatNotifier::agentChosen, this, [this](const QString& id) { chooseAgent(id); });
  // Preferences changed the API agents or the custom command: the
  // switcher's agents, and the chosen one looked for again with its
  // address and key, or its command.
  connect(&ChatNotifier::instance(), &ChatNotifier::apiSettingsChanged, this, [this] {
    noteDetected();
    if (m_agent->configuredInPreferences() && m_activity == Activity::Idle && !m_checkingAccount) {
      m_agent->shutdown();
      m_marker.clear();
      m_agent->check();
    }
    emit setupChanged();
  });
}

void ChatController::adopt(ChatAgent* candidate) {
  connect(candidate, &ChatAgent::modelsChanged, this, [this, candidate] {
    if (candidate == m_agent) {
      emit setupChanged();
    }
  });
  connect(candidate, &ChatAgent::readinessChanged, this, [this, candidate] {
    if (candidate == m_agent) {
      qInfo().noquote() << QStringLiteral("Chat state: %1").arg(readinessName(m_agent->readiness()));
      emit setupChanged();
      settleAccountCheck(false);
    } else {
      emit setupChanged(); // its state in the switcher (mitcad#1289)
    }
  });
  connect(candidate, &ChatAgent::panelEvent, this, [this, candidate](const QJsonObject& event) {
    if (candidate == m_agent) {
      onEvent(event);
    }
  });
  connect(candidate, &ChatAgent::exited, this, [this, candidate](int code, const QString& errors, bool stopped) {
    if (candidate == m_agent) {
      onExited(code, errors, stopped);
    }
  });
}

void ChatController::addClaudeCodes() {
  const QString first = QFileInfo(mcp::assistantExecutable(QStringLiteral("claude"))).canonicalFilePath();
  const QString testHome =
      qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") ? QString() : qEnvironmentVariable("MITCAD_TEST_ASSISTANT_HOME");
  const QString home = testHome.isEmpty() ? QDir::homePath() : testHome;
  // After the Claude Codes already there, before Codex.
  auto at = std::find_if(m_agents.begin(), m_agents.end(),
                         [](const ChatAgent* candidate) { return !qobject_cast<const ClaudeCodeAgent*>(candidate); });
  const bool adopting = m_agent != nullptr; // later looks; the constructor adopts every agent itself
  for (const QString& program : mcp::assistantExecutables(QStringLiteral("claude"))) {
    if (program == first || m_claudePrograms.contains(program)) {
      continue;
    }
    m_claudePrograms << program;
    const QString digest =
        QString::fromLatin1(QCryptographicHash::hash(program.toUtf8(), QCryptographicHash::Sha256).toHex().left(8));
    // The one Claude Desktop keeps (claude-code/<version>/claude).
    const QDir folder = QFileInfo(program).absoluteDir();
    const bool desktop = QFileInfo(folder.absolutePath()).dir().dirName() == QStringLiteral("claude-code");
    const QString place = desktop ? tr("Claude Desktop's") : mcp::assistantPlace(program, home);
    auto* found = new ClaudeCodeAgent(program, QStringLiteral("claude-") + digest, place, this);
    at = m_agents.insert(at, found) + 1;
    qInfo().noquote() << QStringLiteral("Chat: another Claude Code at %1").arg(QDir::toNativeSeparators(program));
    if (adopting) {
      adopt(found);
    }
  }
}

QString ChatController::choiceName(const ChatAgent* candidate) {
  return candidate->place().isEmpty() ? candidate->name() : tr("%1 (%2)").arg(candidate->name(), candidate->place());
}

QString ChatController::choiceState(const ChatAgent* candidate) const {
  QString state;
  switch (candidate->readiness()) {
  case ChatAgent::Readiness::Checking: state = tr("checking"); break;
  case ChatAgent::Readiness::Missing: state = tr("not found"); break;
  case ChatAgent::Readiness::Outdated: state = tr("needs an update"); break;
  case ChatAgent::Readiness::SignedOut: state = candidate->asksForKey() ? tr("needs an API key") : tr("not signed in"); break;
  case ChatAgent::Readiness::SigningIn: state = tr("signing in"); break;
  case ChatAgent::Readiness::Ready:
    // Only the agents with a sign-in of their own are signed in.
    if (candidate->configuredInPreferences()) {
      state = tr("ready");
    } else if (candidate->account().isEmpty()) {
      state = tr("signed in");
    } else {
      state = tr("signed in as %1").arg(candidate->account());
    }
    break;
  }
  const QJsonObject failure = m_accountErrors.value(candidate->id());
  if (!failure.isEmpty()) {
    state += QStringLiteral(" · ") + failure.value(QStringLiteral("reason")).toString();
  }
  return state;
}

ChatAgent* ChatController::readyAlternative() const {
  for (ChatAgent* candidate : detectedAgents()) {
    if (candidate != m_agent && candidate->readiness() == ChatAgent::Readiness::Ready &&
        !m_accountErrors.contains(candidate->id())) {
      return candidate;
    }
  }
  return nullptr;
}

void ChatController::noteDetected() {
  addClaudeCodes();
  QStringList found;
  for (ChatAgent* candidate : m_agents) {
    if (candidate->installedNow()) {
      found << candidate->id();
    }
  }
  if (found != m_detected) {
    m_detected = found;
    qInfo().noquote() << QStringLiteral("Chat: agents found: %1")
                             .arg(found.isEmpty() ? QStringLiteral("none") : found.join(QStringLiteral(", ")));
  }
}

QList<ChatAgent*> ChatController::detectedAgents() const {
  QList<ChatAgent*> shown;
  for (ChatAgent* candidate : m_agents) {
    if (candidate == m_agent || m_detected.contains(candidate->id())) {
      shown.append(candidate);
    }
  }
  return shown;
}

QString ChatController::chosenModel() const { return chatModel(m_agent->id()); }

void ChatController::setModel(const QString& name) {
  const QString chosen = name.trimmed();
  if (chosen == chosenModel()) {
    return;
  }
  setChatModel(m_agent->id(), chosen);
  qInfo().noquote() << QStringLiteral("Chat: model %1 for %2")
                           .arg(chosen.isEmpty() ? QStringLiteral("default") : chosen, m_agent->name());
  if (m_activity == Activity::Idle && m_agent->running()) {
    // The next message starts the agent again with the model; the session
    // goes on.
    m_agent->shutdown();
    m_marker.clear();
  }
  emit setupChanged();
}

ChatController::~ChatController() {
  m_cards.clear();
  for (ChatAgent* candidate : m_agents) {
    candidate->disconnect(this);
    candidate->shutdown();
  }
}

void ChatController::chooseAgent(const QString& id) {
  if (m_agent->id() == id || m_activity != Activity::Idle || m_checkingAccount) {
    return;
  }
  for (ChatAgent* candidate : m_agents) {
    if (candidate->id() != id) {
      continue;
    }
    closeAnswer();
    m_agent->shutdown();
    m_marker.clear();
    m_agent = candidate;
    ChatSettings settings = ChatSettings::load();
    settings.agent = id;
    settings.save();
    m_model.clear();
    qInfo().noquote() << QStringLiteral("Chat: agent %1").arg(choiceName(m_agent));
    // A new conversation in it: the other agent's is not its own.
    startOver(true);
    m_agent->check();
    return;
  }
}

QString ChatController::unavailable() const {
  QString reason;
  if (!m_host.listener(&reason).isEmpty()) {
    return QString();
  }
  return reason.isEmpty() ? tr("MitCad's listener for assistants is not running.") : reason;
}

void ChatController::refreshSetup(bool full) {
  const QStringList before = m_detected;
  noteDetected();
  if (m_detected != before) {
    emit setupChanged(); // the switcher's agents
  }
  if (m_activity != Activity::Idle || m_checkingAccount) {
    return;
  }
  if (full) {
    m_agent->check();
    // The others found too: their sign-in shows in the switcher before one
    // is chosen (mitcad#1289). Not the custom command, which is started
    // only once chosen.
    for (ChatAgent* candidate : detectedAgents()) {
      if (candidate != m_agent && !candidate->running() && qobject_cast<AcpAgent*>(candidate) == nullptr) {
        candidate->check();
      }
    }
  } else if (m_agent->readiness() == ChatAgent::Readiness::Missing && m_agent->installedNow()) {
    // Installed meanwhile: on without a restart.
    qInfo().noquote() << QStringLiteral("Chat: %1 appeared").arg(m_agent->name());
    m_agent->check();
  }
}

void ChatController::signIn() {
  m_agent->signIn();
}

void ChatController::setApiKey(const QString& key) {
  if (!key.trimmed().isEmpty()) {
    m_agent->setKey(key);
  }
}

void ChatController::accountFailed(const QJsonObject& account, const QString& text) {
  const bool repeated = m_accountErrors.value(m_agent->id()) == account;
  m_accountErrors.insert(m_agent->id(), account);
  if (m_checkingAccount) {
    m_checkSawAccountError = true;
  }
  qInfo().noquote() << QStringLiteral("Chat: %1 account error: %2")
                           .arg(m_agent->name(), account.value(QStringLiteral("code")).toString());
  if (!m_checkingAccount && !repeated) {
    const QString shown = text.isEmpty() ? account.value(QStringLiteral("reason")).toString() : text;
    if (m_answer >= 0 && m_entries[m_answer].value(QStringLiteral("text")).toString().trimmed() == shown.trimmed()) {
      QJsonObject& entry = m_entries[m_answer];
      entry.insert(QStringLiteral("role"), QStringLiteral("notice"));
      entry.insert(QStringLiteral("level"), QStringLiteral("error"));
      emit entryChanged(m_answer);
      m_answer = -1;
      record(entry);
      qInfo().noquote() << QStringLiteral("Chat notice (error): %1").arg(shown);
    } else {
      notice(shown, NoticeLevel::Error);
    }
  }
  // The waiting messages are not sent with this account; said as Stop says
  // it (mitcad#1155), before the turn ends.
  if (!m_queue.isEmpty()) {
    notice(tr("%n waiting message(s) were not sent.", nullptr, int(m_queue.size())), NoticeLevel::Warning);
    m_queue.clear();
  }
  if (!m_checkingAccount && m_turn.active) {
    // Some agents emit an error notification and then keep retrying. Let
    // a buffered turn_end arrive first; otherwise end that turn so the
    // explicit account check is available. No more model calls may run.
    const QString marker = m_marker;
    QTimer::singleShot(1000, this, [this, marker] {
      if (marker == m_marker && m_turn.active &&
          (m_activity == Activity::Working || m_activity == Activity::Starting)) {
        stop();
      }
    });
  }
  emit setupChanged();
}

void ChatController::checkAccount() {
  if (m_activity != Activity::Idle || m_checkingAccount) {
    return;
  }
  m_checkingAccount = true;
  m_accountProbe = false;
  m_checkSawAccountError = false;
  ++m_probeGeneration;
  const int generation = m_probeGeneration;
  m_agent->shutdown();
  m_marker.clear();
  qInfo().noquote() << QStringLiteral("Chat: checking %1 account").arg(m_agent->name());
  emit setupChanged();
  m_agent->check();
  // A check that emits no readiness change (mitcad#1155): one that ends at
  // once with the readiness it had (the program is gone, as at the last
  // look) is settled here; one that keeps an unfinished sign-in may never
  // change it, so the setup check is bounded too.
  settleAccountCheck(true);
  QTimer::singleShot(kAccountSetupCheckMs, this, [this, generation] {
    if (m_checkingAccount && !m_accountProbe && generation == m_probeGeneration) {
      finishAccountProbe(false, m_agent->readiness() == ChatAgent::Readiness::SigningIn
                                    ? readinessProblem()
                                    : tr("The account check timed out. Try again."));
    }
  });
}

void ChatController::settleAccountCheck(bool atOnce) {
  if (!m_checkingAccount || m_accountProbe) {
    return;
  }
  switch (accountCheckStep(m_agent->readiness(), atOnce)) {
  case AccountCheckStep::Wait: break;
  case AccountCheckStep::Probe: beginAccountProbe(); break;
  case AccountCheckStep::Fail: finishAccountProbe(false, readinessProblem()); break;
  }
}

QString ChatController::readinessProblem() const {
  const QString detail = m_agent->readinessDetail();
  switch (m_agent->readiness()) {
  case ChatAgent::Readiness::Missing:
    return detail.isEmpty() ? tr("%1 was not found.").arg(m_agent->name()) : detail;
  case ChatAgent::Readiness::Outdated:
    return detail.isEmpty() ? tr("%1 needs an update.").arg(m_agent->name()) : detail;
  case ChatAgent::Readiness::Checking:
  case ChatAgent::Readiness::SignedOut:
  case ChatAgent::Readiness::SigningIn:
  case ChatAgent::Readiness::Ready:
    break;
  }
  return tr("Sign in to %1 and check again.").arg(m_agent->accountName());
}

void ChatController::beginAccountProbe() {
  QString reason;
  const QString instance = m_host.listener(&reason);
  const QString bridge = mcp::installedBridge(QCoreApplication::applicationDirPath());
  if (instance.isEmpty() || (m_agent->needsBridge() && bridge.isEmpty())) {
    finishAccountProbe(false, reason.isEmpty() ? tr("MitCad's bridge is unavailable.") : reason);
    return;
  }
  m_accountProbe = true;
  m_marker = QUuid::createUuid().toString(QUuid::Id128);
  const ChatAgent::Start start{newSession(), false, bridge, instance, m_marker, chosenModel()};
  setActivity(Activity::Starting);
  if (!m_agent->start(start, &reason) ||
      !m_agent->send({QStringLiteral("Reply with OK only. Do not use tools."), QString(), {}}, &reason)) {
    finishAccountProbe(false, reason);
    return;
  }
  const int generation = m_probeGeneration;
  QTimer::singleShot(30000, this, [this, generation] {
    if (m_checkingAccount && generation == m_probeGeneration) {
      finishAccountProbe(false, tr("The account check timed out. Try again."));
    }
  });
}

void ChatController::finishAccountProbe(bool success, const QString& reason) {
  if (!m_checkingAccount) {
    return;
  }
  m_checkingAccount = false;
  m_accountProbe = false;
  ++m_probeGeneration;
  m_marker.clear();
  if (success) {
    m_accountErrors.remove(m_agent->id());
    qInfo().noquote() << QStringLiteral("Chat: %1 account check passed").arg(m_agent->name());
  } else {
    // The account's own reason stays in the state line (mitcad#1155); why
    // this check failed otherwise (the network, the agent gone, a timeout)
    // is a notice beside it, not kept in the history.
    const QString kept = m_accountErrors.value(m_agent->id()).value(QStringLiteral("reason")).toString();
    qInfo().noquote() << QStringLiteral("Chat: %1 account check failed: %2")
                             .arg(m_agent->name(), reason.isEmpty() ? kept : reason);
    if (!reason.isEmpty() && !m_checkSawAccountError) {
      notice(tr("The account check failed: %1").arg(reason), NoticeLevel::Warning, false);
    }
  }
  // feed_line has returned before panelEvent is emitted. Both process
  // adapters stop their event loop when shutdown detaches the process.
  // Detach before reporting Ready, so a new message cannot reuse the
  // probe's process or temporary session.
  m_agent->shutdown();
  setActivity(Activity::Idle);
  emit setupChanged();
}

void ChatController::openInstallPage() {
  qInfo().noquote() << QStringLiteral("Chat: install page of %1").arg(m_agent->name());
  m_host.openUrl(m_agent->installPage());
}

bool ChatController::needsPrivacyNotice() const { return !privacyNoticeShown(m_agent->id()); }

void ChatController::acceptPrivacyNotice() {
  setPrivacyNoticeShown(m_agent->id());
  qInfo().noquote() << QStringLiteral("Chat: privacy notice accepted for %1").arg(m_agent->name());
}

void ChatController::setActivity(Activity now) {
  if (now == m_activity) {
    return;
  }
  m_activity = now;
  qInfo().noquote() << QStringLiteral("Chat activity: %1").arg(activityName(now));
  m_host.setWorking(now == Activity::Idle ? QString() : m_agent->name());
  emit activityChanged();
}

int ChatController::addEntry(const QJsonObject& entry) {
  m_entries.append(entry);
  const int index = int(m_entries.size()) - 1;
  emit entryAdded(index);
  return index;
}

void ChatController::notice(const QString& text, NoticeLevel level, bool keep) {
  closeAnswer();
  QJsonObject entry{{QStringLiteral("role"), QStringLiteral("notice")}, {QStringLiteral("text"), text}};
  // Its level (mitcad#1040): an error does not look like information.
  if (level != NoticeLevel::Information) {
    entry.insert(QStringLiteral("level"), noticeLevelName(level));
  }
  addEntry(entry);
  if (level == NoticeLevel::Information) {
    qInfo().noquote() << QStringLiteral("Chat notice: %1").arg(text);
  } else {
    qInfo().noquote() << QStringLiteral("Chat notice (%1): %2").arg(noticeLevelName(level), text);
  }
  if (keep) {
    record(entry);
  }
}

int ChatController::toolIndex(const QString& id) const {
  // Only the running process's calls: a call id is unique only within the
  // agent's process, and a new process (after Stop on Windows, a crash or a
  // resumed session) may number its calls from the start again.
  for (int index = int(m_entries.size()) - 1; index >= m_toolsFrom; --index) {
    const QJsonObject& entry = m_entries[index];
    if (entry.value(QStringLiteral("role")).toString() == QStringLiteral("tool") &&
        entry.value(QStringLiteral("id")).toString() == id) {
      return index;
    }
  }
  return -1;
}

void ChatController::record(QJsonObject entry) {
  if (!ChatSettings::load().history) {
    return;
  }
  entry.insert(QStringLiteral("time"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
  entry.insert(QStringLiteral("agent"), m_agent->id());
  if (!m_session.isEmpty()) {
    entry.insert(QStringLiteral("session"), m_session);
  }
  const QJsonObject line =
      chatRequest({{QStringLiteral("op"), QStringLiteral("history_entry")}, {QStringLiteral("entry"), entry}});
  if (line.contains(QStringLiteral("error"))) {
    qWarning().noquote() << QStringLiteral("Chat: a history entry was not kept: %1")
                                .arg(line.value(QStringLiteral("error")).toString());
    return;
  }
  const QByteArray bytes = line.value(QStringLiteral("line")).toString().toUtf8();
  if (m_path.isEmpty()) {
    // An untitled design: until it is saved.
    if (m_unsaved.size() + bytes.size() <= kUnsavedLimit) {
      m_unsaved += bytes;
    }
  } else if (!m_store.append(m_path, bytes)) {
    qWarning().noquote() << QStringLiteral("Chat: the history could not be written to %1")
                                .arg(QDir::toNativeSeparators(m_store.directory()));
  }
}

void ChatController::loadHistory() {
  m_entries.clear();
  m_toolsFrom = 0;
  m_answer = -1;
  m_sessions = QJsonObject();
  m_session.clear();
  m_resumable = false;
  if (m_path.isEmpty() || !ChatSettings::load().history) {
    return;
  }
  const QByteArray content = m_store.read(m_path);
  if (content.isEmpty()) {
    return;
  }
  const QJsonObject read = chatRequest({{QStringLiteral("op"), QStringLiteral("history_read")},
                                        {QStringLiteral("content"), QString::fromUtf8(content)}});
  for (const QJsonValue& value : read.value(QStringLiteral("entries")).toArray()) {
    QJsonObject entry = value.toObject();
    const QString role = entry.value(QStringLiteral("role")).toString();
    if (role == QStringLiteral("tool")) {
      entry.insert(QStringLiteral("state"), entry.value(QStringLiteral("error")).toBool() ? QStringLiteral("failed")
                                                                                          : QStringLiteral("done"));
    }
    if (role == QStringLiteral("notice") && entry.value(QStringLiteral("event")).toString() == QStringLiteral("new_conversation")) {
      continue;
    }
    m_entries.append(entry);
  }
  m_sessions = read.value(QStringLiteral("sessions")).toObject();
  m_session = m_sessions.value(m_agent->id()).toString();
  m_resumable = !m_session.isEmpty();
  qInfo().noquote() << QStringLiteral("Chat: history of %1 entries shown%2")
                           .arg(m_entries.size())
                           .arg(m_session.isEmpty() ? QString() : QStringLiteral(", session %1 to resume").arg(m_session));
}

void ChatController::documentChanged(const QString& documentId, const QString& path) {
  if (documentId == m_documentId) {
    if (path == m_path) {
      return;
    }
    if (m_path.isEmpty()) {
      // Saved for the first time: the conversation is the design's now.
      if (ChatSettings::load().history && !m_unsaved.isEmpty() && !m_store.append(path, m_unsaved)) {
        qWarning().noquote() << QStringLiteral("Chat: the history could not be written");
      }
      m_unsaved.clear();
    } else if (!m_store.move(m_path, path)) {
      qWarning().noquote() << QStringLiteral("Chat: the history did not move with the design");
    }
    qInfo().noquote() << QStringLiteral("Chat: the conversation follows the design to %1").arg(QDir::toNativeSeparators(path));
    m_path = path;
    return;
  }
  if (m_inTool) {
    // The agent opened another design (document_open): the conversation
    // goes on with it; the message's steps went with the design before.
    qInfo().noquote() << QStringLiteral("Chat: the agent opened %1; the conversation goes on with it")
                             .arg(path.isEmpty() ? QStringLiteral("a design") : QDir::toNativeSeparators(path));
    m_documentId = documentId;
    m_path = path;
    m_unsaved.clear();
    m_turn.startDepth = -1;
    return;
  }
  if (m_checkingAccount) {
    finishAccountProbe(false);
  }
  // Another design: its own conversation. The agent of the one before ends
  // (its undo steps went with its document).
  m_documentId = documentId;
  m_path = path;
  m_unsaved.clear();
  m_queue.clear();
  m_cards.clear();
  m_turn = Turn();
  m_agent->shutdown();
  m_marker.clear();
  m_model.clear();
  setActivity(Activity::Idle);
  loadHistory();
  emit conversationReset();
  emit setupChanged();
}

void ChatController::startOver(bool mark) {
  if (m_checkingAccount) {
    finishAccountProbe(false);
  }
  closeAnswer();
  m_agent->shutdown();
  m_marker.clear();
  m_cards.clear();
  m_queue.clear();
  m_turn = Turn();
  setActivity(Activity::Idle);
  m_session = newSession();
  m_resumable = false;
  m_sessions = QJsonObject();
  m_entries.clear();
  m_toolsFrom = 0;
  m_answer = -1;
  if (mark) {
    record({{QStringLiteral("role"), QStringLiteral("notice")},
            {QStringLiteral("event"), QStringLiteral("new_conversation")},
            {QStringLiteral("text"), QStringLiteral("New conversation")}});
  }
  qInfo().noquote() << QStringLiteral("Chat: new conversation (session %1)").arg(m_session);
  emit conversationReset();
  emit setupChanged();
}

void ChatController::newConversation() {
  startOver(true);
}

void ChatController::clearHistory(bool all) {
  // What the API agents kept of the conversations (their sessions) goes
  // with the history.
  if (all) {
    clearApiSessions();
  } else {
    for (ChatAgent* candidate : m_agents) {
      const QString kept = m_sessions.value(candidate->id()).toString();
      if (!kept.isEmpty()) {
        candidate->forgetSession(kept);
      }
    }
    if (!m_session.isEmpty()) {
      m_agent->forgetSession(m_session);
    }
  }
  const bool cleared = all ? m_store.clearAll() : m_store.clear(m_path);
  m_unsaved.clear();
  qInfo().noquote() << QStringLiteral("Chat: history of %1 cleared%2")
                           .arg(all ? QStringLiteral("every design") : QStringLiteral("this design"))
                           .arg(cleared ? QString() : QStringLiteral(" (not all of it could be removed)"));
  startOver(false);
}

void ChatController::send(const QString& text, bool attachSelection, bool attachView, bool attachComments) {
  if (!accountError().isEmpty() || m_checkingAccount) {
    return;
  }
  const QString shown = text.trimmed();
  if (shown.isEmpty()) {
    return;
  }
  ChatAgent::Message message;
  message.text = shown;
  QStringList attached;
  if (attachSelection) {
    const QJsonArray selection = m_host.selection();
    if (!selection.isEmpty()) {
      // For the agent: the entities as ui_state and ui_select give them.
      message.context = QStringLiteral("Selected in MitCad (as ui_state's selection): ") + compact(selection, 32000);
      attached << QStringLiteral("%1 selected").arg(selection.size());
    }
  }
  // The open comments (mitcad#1119), as the comments resource lists them,
  // so the agent addresses them.
  if (attachComments && m_host.comments) {
    const QJsonArray comments = m_host.comments();
    if (!comments.isEmpty()) {
      const QString listed = QStringLiteral("Open comments in MitCad (address each with address_comment in the same "
                                            "model_command array as its change): ") +
                             compact(comments, 32000);
      message.context = message.context.isEmpty() ? listed : message.context + QStringLiteral("\n\n") + listed;
      attached << QStringLiteral("%1 open comments").arg(comments.size());
    }
  }
  if (attachView) {
    const QByteArray picture = m_host.viewPicture();
    if (!picture.isEmpty()) {
      message.pictures.append(ChatAgent::Picture{QStringLiteral("image/png"), picture});
      attached << QStringLiteral("view");
    }
  }
  qInfo().noquote() << QStringLiteral("Chat: message written (%1 characters%2)")
                           .arg(shown.size())
                           .arg(attached.isEmpty() ? QString() : QStringLiteral(", ") + attached.join(QStringLiteral(", ")));
  if (m_activity != Activity::Idle) {
    m_queue.append(std::make_pair(message, shown));
    qInfo().noquote() << QStringLiteral("Chat: message queued (%1 waiting)").arg(m_queue.size());
    emit setupChanged();
    return;
  }
  dispatch(message, shown);
}

bool ChatController::startAgent(QString* error) {
  QString reason;
  const QString instance = m_host.listener(&reason);
  if (instance.isEmpty()) {
    *error = reason.isEmpty() ? tr("MitCad's listener for assistants is not running.") : reason;
    return false;
  }
  // An agent whose tool loop runs in MitCad connects to the listener
  // itself (ApiAgent).
  const QString bridge = m_agent->needsBridge() ? mcp::bundledBridge() : QString();
  if (m_agent->needsBridge() && bridge.isEmpty()) {
    *error = tr("MitCad's mitcad-cli was not found beside the application.");
    return false;
  }
  if (m_session.isEmpty()) {
    m_session = newSession();
    m_resumable = false;
  }
  m_marker = QUuid::createUuid().toString(QUuid::Id128);
  const ChatAgent::Start start{m_session, m_resumable, bridge, instance, m_marker, chosenModel()};
  if (!m_agent->start(start, error)) {
    m_marker.clear();
    return false;
  }
  m_resuming = m_resumable;
  m_sawInit = false;
  m_toolsFrom = int(m_entries.size());
  m_model.clear();
  setActivity(Activity::Starting);
  return true;
}

void ChatController::dispatch(const ChatAgent::Message& message, const QString& shown, bool retry) {
  QString error;
  if (!m_agent->running() && !startAgent(&error)) {
    notice(tr("The message was not sent: %1").arg(error), NoticeLevel::Error);
    setActivity(Activity::Idle);
    return;
  }
  m_inFlight = {message, shown};
  const QJsonObject state = m_host.documentState();
  m_turn = Turn();
  m_turn.active = true;
  m_turn.text = shown;
  m_turn.startDepth = state.isEmpty() ? -1 : state.value(QStringLiteral("undo_depth")).toInt();
  m_turn.lastState = state;
  m_turn.startSketch = state.value(QStringLiteral("sketch")).toBool();
  if (!m_agent->send(message, &error)) {
    m_turn.active = false;
    notice(tr("The message was not sent: %1").arg(error), NoticeLevel::Error);
    return;
  }
  ++m_messages;
  if (!retry) {
    closeAnswer();
    addEntry({{QStringLiteral("role"), QStringLiteral("user")}, {QStringLiteral("text"), shown}});
    record({{QStringLiteral("role"), QStringLiteral("user")}, {QStringLiteral("text"), shown}});
  }
  qInfo().noquote() << QStringLiteral("Chat: message %1 sent (undo depth %2)").arg(m_messages).arg(m_turn.startDepth);
  if (m_activity != Activity::Starting) {
    setActivity(Activity::Working);
  }
  emit setupChanged();
}

void ChatController::closeAnswer() {
  if (m_answer < 0 || m_answer >= m_entries.size()) {
    m_answer = -1;
    return;
  }
  const QString text = m_entries[m_answer].value(QStringLiteral("text")).toString();
  m_answer = -1;
  if (!text.isEmpty()) {
    record({{QStringLiteral("role"), QStringLiteral("assistant")}, {QStringLiteral("text"), text}});
  }
}

void ChatController::onEvent(const QJsonObject& event) {
  const QString kind = event.value(QStringLiteral("kind")).toString();
  if (kind == QStringLiteral("account_error")) {
    accountFailed(event.value(QStringLiteral("account")).toObject(), event.value(QStringLiteral("text")).toString());
    return;
  }
  if (m_accountProbe) {
    if (kind == QStringLiteral("init")) {
      setActivity(Activity::Working);
    } else if (kind == QStringLiteral("turn_end")) {
      const bool success = !event.value(QStringLiteral("error")).toBool();
      finishAccountProbe(success, success || event.value(QStringLiteral("account_error")).toBool()
                                     ? QString() : event.value(QStringLiteral("text")).toString());
    }
    return;
  }
  if (kind == QStringLiteral("init")) {
    m_sawInit = true;
    m_resumable = true;
    m_model = event.value(QStringLiteral("model")).toString();
    const QString bridge = event.value(QStringLiteral("bridge")).toString();
    const QString session = event.value(QStringLiteral("session")).toString();
    qInfo().noquote() << QStringLiteral("Chat: %1 started its session %2, model %3, MitCad's tools %4")
                             .arg(m_agent->name(), session, m_model, bridge);
    if (!session.isEmpty() && session != m_session) {
      // An agent that names its sessions itself (Codex's threads): the
      // conversation goes on with that one.
      m_session = session;
    }
    if (event.value(QStringLiteral("resume_failed")).toBool()) {
      notice(tr("%1 could not go on with the earlier conversation; a new one started.").arg(m_agent->name()),
             NoticeLevel::Warning);
    }
    if (bridge != QStringLiteral("connected") && bridge != QStringLiteral("pending")) {
      const QString why = event.value(QStringLiteral("bridge_error")).toString();
      notice(tr("MitCad's tools did not start in %1: %2")
                 .arg(m_agent->name(), why.isEmpty() ? tr("the bridge is %1").arg(bridge) : why),
             NoticeLevel::Error);
    }
    if (m_activity == Activity::Starting) {
      setActivity(m_turn.active ? Activity::Working : Activity::Idle);
    }
    emit setupChanged();
  } else if (kind == QStringLiteral("text")) {
    if (m_answer < 0) {
      m_answer = addEntry({{QStringLiteral("role"), QStringLiteral("assistant")}, {QStringLiteral("text"), QString()}});
    }
    QJsonObject& streamed = m_entries[m_answer];
    streamed.insert(QStringLiteral("text"),
                  streamed.value(QStringLiteral("text")).toString() + event.value(QStringLiteral("text")).toString());
    emit entryChanged(m_answer);
  } else if (kind == QStringLiteral("tool_start") || kind == QStringLiteral("tool")) {
    closeAnswer();
    const QString id = event.value(QStringLiteral("id")).toString();
    const QString name = event.value(QStringLiteral("name")).toString();
    int index = toolIndex(id);
    if (index < 0) {
      index = addEntry({{QStringLiteral("role"), QStringLiteral("tool")},
                        {QStringLiteral("id"), id},
                        {QStringLiteral("name"), name},
                        {QStringLiteral("state"), QStringLiteral("running")}});
    }
    if (kind == QStringLiteral("tool")) {
      const QJsonObject input = event.value(QStringLiteral("input")).toObject();
      QJsonObject& tool = m_entries[index];
      tool.insert(QStringLiteral("summary"), toolSummary(name, input));
      tool.insert(QStringLiteral("input"), compact(input, 2000));
      emit entryChanged(index);
      qInfo().noquote() << QStringLiteral("Chat: tool %1 %2").arg(name, tool.value(QStringLiteral("summary")).toString());
    }
  } else if (kind == QStringLiteral("tool_result")) {
    const int index = toolIndex(event.value(QStringLiteral("id")).toString());
    if (index < 0) {
      return;
    }
    const bool failed = event.value(QStringLiteral("error")).toBool();
    QJsonObject& tool = m_entries[index];
    tool.insert(QStringLiteral("state"), failed ? QStringLiteral("failed") : QStringLiteral("done"));
    tool.insert(QStringLiteral("result"), event.value(QStringLiteral("text")).toString());
    emit entryChanged(index);
    qInfo().noquote() << QStringLiteral("Chat: tool %1 %2")
                             .arg(tool.value(QStringLiteral("name")).toString(),
                                  failed ? QStringLiteral("failed") : QStringLiteral("done"));
    record({{QStringLiteral("role"), QStringLiteral("tool")},
            {QStringLiteral("name"), tool.value(QStringLiteral("name")).toString()},
            {QStringLiteral("summary"), tool.value(QStringLiteral("summary")).toString()},
            {QStringLiteral("error"), failed}});
  } else if (kind == QStringLiteral("turn_end")) {
    closeAnswer();
    const bool stopped = m_activity == Activity::Stopping;
    const QString subtype = event.value(QStringLiteral("subtype")).toString();
    qInfo().noquote() << QStringLiteral("Chat: turn ended (%1)").arg(subtype);
    if (stopped) {
      notice(tr("Stopped."));
    } else if (event.value(QStringLiteral("error")).toBool() && !event.value(QStringLiteral("account_error")).toBool()) {
      const QString text = event.value(QStringLiteral("text")).toString();
      notice(text.isEmpty() ? tr("%1 ended the message with an error (%2).").arg(m_agent->name(), subtype) : text,
             NoticeLevel::Error);
    }
    endTurn(true);
    setActivity(Activity::Idle);
    if (!m_queue.isEmpty()) {
      const auto next = m_queue.takeFirst();
      dispatch(next.first, next.second);
    }
    emit setupChanged();
  } else if (kind == QStringLiteral("bridge")) {
    // MitCad's tools after the start (Codex starts its MCP servers then).
    const QString bridge = event.value(QStringLiteral("bridge")).toString();
    qInfo().noquote() << QStringLiteral("Chat: MitCad's tools %1 in %2").arg(bridge, m_agent->name());
    if (bridge == QStringLiteral("failed")) {
      const QString why = event.value(QStringLiteral("bridge_error")).toString();
      notice(tr("MitCad's tools did not start in %1: %2")
                 .arg(m_agent->name(), why.isEmpty() ? tr("the bridge is %1").arg(bridge) : why),
             NoticeLevel::Error);
    }
  } else if (kind == QStringLiteral("status")) {
    qInfo().noquote() << QStringLiteral("Chat: %1 says %2 %3")
                             .arg(m_agent->name(), event.value(QStringLiteral("subtype")).toString(),
                                  event.value(QStringLiteral("text")).toString());
  } else if (kind == QStringLiteral("output")) {
    qDebug().noquote() << QStringLiteral("Chat: %1 wrote: %2").arg(m_agent->name(), event.value(QStringLiteral("text")).toString());
  }
}

void ChatController::onExited(int code, const QString& errors, bool stopped) {
  if (m_checkingAccount) {
    finishAccountProbe(false, errors.isEmpty() ? tr("The agent ended before the account check finished.") : errors);
    return;
  }
  const QJsonObject account = chatRequest({{QStringLiteral("op"), QStringLiteral("account_error")},
                                          {QStringLiteral("text"), errors}});
  if (!stopped && !account.isEmpty()) {
    accountFailed(account, errors);
  }
  m_marker.clear();
  for (int index = 0; index < m_entries.size(); ++index) {
    if (m_entries[index].value(QStringLiteral("role")).toString() == QStringLiteral("card") &&
        m_entries[index].value(QStringLiteral("state")).toString() == QStringLiteral("open")) {
      closeCard(index, QStringLiteral("gone"));
    }
  }
  m_cards.clear();
  if (m_turn.active) {
    closeAnswer();
    if (m_resuming && !m_sawInit && !stopped) {
      // The session could not be resumed (the agent no longer has it): a
      // new one, and the message again.
      m_turn.active = false;
      m_session = newSession();
      m_resumable = false;
      m_resuming = false;
      setActivity(Activity::Idle);
      notice(tr("%1 could not go on with the earlier conversation; a new one started.").arg(m_agent->name()),
             NoticeLevel::Warning);
      dispatch(m_inFlight.first, m_inFlight.second, true);
      return;
    }
    if (stopped) {
      notice(tr("Stopped."));
    } else if (accountError().isEmpty()) {
      notice(tr("%1 ended: %2").arg(m_agent->name(), errors.isEmpty() ? tr("exit status %1").arg(code) : errors), NoticeLevel::Error);
    }
    endTurn(true);
  } else if (!stopped && !m_sawInit && accountError().isEmpty()) {
    notice(tr("%1 ended: %2").arg(m_agent->name(), errors.isEmpty() ? tr("exit status %1").arg(code) : errors), NoticeLevel::Error);
  }
  if (!m_queue.isEmpty()) {
    notice(tr("%n waiting message(s) were not sent.", nullptr, int(m_queue.size())), NoticeLevel::Warning);
    m_queue.clear();
  }
  setActivity(Activity::Idle);
  emit setupChanged();
}

void ChatController::stop() {
  if (m_activity != Activity::Working && m_activity != Activity::Starting) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Chat: Stop");
  denyCards(tr("The user stopped the message."));
  if (!m_queue.isEmpty()) {
    notice(tr("%n waiting message(s) were not sent.", nullptr, int(m_queue.size())), NoticeLevel::Warning);
    m_queue.clear();
  }
  setActivity(Activity::Stopping);
  m_agent->stop();
}

void ChatController::endTurn(bool merge) {
  if (!m_turn.active) {
    return;
  }
  const Turn turn = m_turn;
  m_turn.active = false;
  denyCards(tr("The message ended."));
  if (!merge) {
    return;
  }
  const QJsonObject now = m_host.documentState();
  if (now.isEmpty()) {
    // Computing: once it is done.
    m_host.whenIdle([this, turn] { mergeTurn(turn, m_host.documentState()); });
    return;
  }
  mergeTurn(turn, now);
}

bool ChatController::sameState(const QJsonObject& a, const QJsonObject& b) {
  return !a.isEmpty() && !b.isEmpty() &&
         a.value(QStringLiteral("undo_depth")) == b.value(QStringLiteral("undo_depth")) &&
         a.value(QStringLiteral("revision")) == b.value(QStringLiteral("revision"));
}

void ChatController::mergeTurn(const Turn& turn, const QJsonObject& now) {
  if (now.isEmpty() || turn.startDepth < 0) {
    qInfo().noquote() << QStringLiteral("Chat: the message's steps stay as they are (the design was busy)");
    return;
  }
  const bool edited = turn.userEdited || !sameState(turn.lastState, now);
  const QJsonObject decision = chatRequest({{QStringLiteral("op"), QStringLiteral("undo_merge")},
                                            {QStringLiteral("start_depth"), turn.startDepth},
                                            {QStringLiteral("end_depth"), now.value(QStringLiteral("undo_depth"))},
                                            {QStringLiteral("user_edited"), edited},
                                            {QStringLiteral("sketch"), turn.startSketch || now.value(QStringLiteral("sketch")).toBool()},
                                            {QStringLiteral("panel"), now.value(QStringLiteral("panel")).toBool()},
                                            {QStringLiteral("message"), turn.text}});
  const QString label = decision.value(QStringLiteral("label")).toString();
  if (!decision.value(QStringLiteral("merge")).toBool()) {
    qInfo().noquote() << QStringLiteral("Chat: the message's steps stay separate (%1)")
                             .arg(decision.value(QStringLiteral("reason")).toString());
    return;
  }
  const int depth = decision.value(QStringLiteral("depth")).toInt();
  const int steps = now.value(QStringLiteral("undo_depth")).toInt() - depth;
  if (m_host.mergeUndo(depth, label)) {
    qInfo().noquote() << QStringLiteral("Chat: the message's %1 step(s) merged into '%2'").arg(steps).arg(label);
  } else {
    qWarning().noquote() << QStringLiteral("Chat: the message's steps could not be merged into '%1'").arg(label);
  }
}

void ChatController::closeCard(int index, const QString& outcome) {
  if (index < 0 || index >= m_entries.size()) {
    return;
  }
  m_entries[index].insert(QStringLiteral("state"), outcome);
  emit entryChanged(index);
}

void ChatController::denyCards(const QString& reason) {
  // Moved out first: an answer may end the connection, which clears them.
  std::vector<Card> cards;
  cards.swap(m_cards);
  for (Card& card : cards) {
    for (int index = 0; index < m_entries.size(); ++index) {
      if (m_entries[index].value(QStringLiteral("id")).toString() == card.id) {
        closeCard(index, QStringLiteral("denied"));
      }
    }
    if (card.answer) {
      card.answer(false, reason);
    }
  }
}

void ChatController::answer(const QString& id, Answer choice) {
  const auto found = std::find_if(m_cards.begin(), m_cards.end(), [&id](const Card& card) { return card.id == id; });
  if (found == m_cards.end()) {
    return;
  }
  Card card = std::move(*found);
  m_cards.erase(found);
  const bool allowed = choice != Answer::Deny;
  if (choice == Answer::Always) {
    allowAlways(m_agent->id(), card.kind);
  }
  const QString chosen = choice == Answer::Once     ? QStringLiteral("Allow Once")
                         : choice == Answer::Always ? QStringLiteral("Always Allow")
                                                    : QStringLiteral("Deny");
  qInfo().noquote() << QStringLiteral("Chat: %1 for %2").arg(chosen, card.kind);
  for (int index = 0; index < m_entries.size(); ++index) {
    if (m_entries[index].value(QStringLiteral("id")).toString() == card.id) {
      closeCard(index, choice == Answer::Always ? QStringLiteral("always")
                       : allowed                ? QStringLiteral("allowed")
                                                : QStringLiteral("denied"));
      const QString path = m_entries[index].value(QStringLiteral("path")).toString();
      record({{QStringLiteral("role"), QStringLiteral("notice")},
              {QStringLiteral("text"), QStringLiteral("%1: %2%3").arg(chosen, card.kind,
                                                                      path.isEmpty() ? QString() : QLatin1Char(' ') + path)}});
    }
  }
  if (card.answer) {
    card.answer(allowed, allowed ? QString() : tr("The user did not allow this (%1).").arg(card.kind));
  }
}

mcp::ChatGate::Decision ChatController::decide(const QString& marker, const QString& tool, const QJsonObject& arguments,
                                               std::function<void(bool allowed, const QString& reason)> reply) {
  if (marker.isEmpty() || marker != m_marker) {
    return {Verdict::Deny, tr("This chat's conversation has ended.")};
  }
  if (!m_turn.active) {
    return {Verdict::Deny, tr("No message of the chat is running.")};
  }
  if (!accountError().isEmpty()) {
    return {Verdict::Deny, tr("The agent's account cannot be used. Check it again in the chat panel.")};
  }
  const ChatSettings settings = ChatSettings::load();
  const QJsonObject state = m_host.documentState();
  const QJsonObject verdict = chatRequest(
      {{QStringLiteral("op"), QStringLiteral("approval")},
       {QStringLiteral("tool"), tool},
       {QStringLiteral("arguments"), arguments},
       {QStringLiteral("mode"), approvalModeKey(settings.approvals)},
       {QStringLiteral("always"), QJsonArray::fromStringList(alwaysAllowed(m_agent->id()))},
       {QStringLiteral("turn_depth"), m_turn.startDepth >= 0 ? QJsonValue(m_turn.startDepth) : QJsonValue()},
       {QStringLiteral("undo_depth"), state.contains(QStringLiteral("undo_depth")) ? state.value(QStringLiteral("undo_depth"))
                                                                                    : QJsonValue()}});
  if (verdict.contains(QStringLiteral("error"))) {
    return {Verdict::Deny, verdict.value(QStringLiteral("error")).toString()};
  }
  if (verdict.value(QStringLiteral("verdict")).toString() == QStringLiteral("run")) {
    return {Verdict::Run, QString()};
  }
  // A card in the conversation; the call waits for its answer.
  const QString id = QUuid::createUuid().toString(QUuid::Id128);
  const QString kind = verdict.value(QStringLiteral("kind")).toString();
  const QString path = verdict.value(QStringLiteral("path")).toString();
  m_cards.push_back({id, kind, std::move(reply)});
  closeAnswer();
  addEntry({{QStringLiteral("role"), QStringLiteral("card")},
            {QStringLiteral("id"), id},
            {QStringLiteral("kind"), kind},
            {QStringLiteral("tool"), tool},
            {QStringLiteral("path"), path},
            {QStringLiteral("state"), QStringLiteral("open")}});
  qInfo().noquote() << QStringLiteral("Chat: approval asked for %1%2")
                           .arg(kind, path.isEmpty() ? QString() : QStringLiteral(" ") + QDir::toNativeSeparators(path));
  return {Verdict::Ask, QString()};
}

void ChatController::toolStarted(const QString& marker, const QString& tool) {
  if (marker != m_marker || !m_turn.active) {
    return;
  }
  m_inTool = true;
  const QJsonObject now = m_host.documentState();
  if (!now.isEmpty() && !m_turn.userEdited && !sameState(now, m_turn.lastState)) {
    // Changed between the agent's calls: the user's.
    m_turn.userEdited = true;
    qInfo().noquote() << QStringLiteral("Chat: the design changed between the agent's calls (before %1); the "
                                        "message's steps stay separate")
                             .arg(tool);
  }
}

void ChatController::toolFinished(const QString& marker, const QString& tool, bool error) {
  Q_UNUSED(tool);
  Q_UNUSED(error);
  m_inTool = false;
  if (marker != m_marker || !m_turn.active) {
    return;
  }
  const QJsonObject now = m_host.documentState();
  if (!now.isEmpty()) {
    m_turn.lastState = now;
    return;
  }
  m_host.whenIdle([this] {
    if (m_turn.active) {
      const QJsonObject idle = m_host.documentState();
      if (!idle.isEmpty()) {
        m_turn.lastState = idle;
      }
    }
  });
}

void ChatController::connectionClosed(const QString& marker) {
  if (marker != m_marker && !m_marker.isEmpty()) {
    return;
  }
  for (const Card& card : m_cards) {
    for (int index = 0; index < m_entries.size(); ++index) {
      if (m_entries[index].value(QStringLiteral("id")).toString() == card.id) {
        closeCard(index, QStringLiteral("gone"));
      }
    }
  }
  m_cards.clear();
}

} // namespace mitcad::agent
