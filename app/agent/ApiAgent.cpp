// SPDX-License-Identifier: MIT
#include "ApiAgent.hpp"

#include <cstddef>
#include <exception>
#include <memory>
#include <string>
#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHostAddress>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QRegularExpression>
#include <QSaveFile>
#include <QTcpSocket>
#include <QTimer>
#include <QUrl>
#include <QtLogging>

#include <qtkeychain/keychain.h>

#include "../mcp/LocalEndpoint.hpp"
#include "ChatLogic.hpp"
#include "ChatSettings.hpp"

namespace mitcad::agent {
namespace {

const QString kKeychainService = QStringLiteral("MitCad AI chat");
// The longest line read from MitCad's listener (a picture of the view is a
// few megabytes) and from a service's answer.
constexpr qsizetype kMaxLine = 32 * 1024 * 1024;
// The models list read at most.
constexpr qsizetype kMaxModelsBody = 1024 * 1024;
constexpr int kConnectMs = 5000;

// UI tests keep their keys out of the user's keychain
// (MITCAD_TEST_KEYCHAIN=off): for the run only.
bool keychainOff() { return qEnvironmentVariable("MITCAD_TEST_KEYCHAIN") == QStringLiteral("off"); }

// Keys given in this run where no keychain keeps them.
QHash<QString, QString>& sessionKeys() {
  static QHash<QString, QString> keys;
  return keys;
}

QJsonArray eventsOf(const std::string& text) {
  const QJsonDocument document = QJsonDocument::fromJson(QByteArray::fromStdString(text));
  if (document.isArray()) {
    return document.array();
  }
  QJsonArray events;
  events.append(QJsonObject{{QStringLiteral("kind"), QStringLiteral("refused")},
                            {QStringLiteral("text"), document.object().value(QStringLiteral("error")).toString()}});
  return events;
}

bool plainSession(const QString& session) {
  static const QRegularExpression plain(QStringLiteral("^[A-Za-z0-9_-]{1,128}$"));
  return plain.match(session).hasMatch();
}

bool privateFolder(const QString& path) {
  if (path.isEmpty() || !QDir().mkpath(path)) {
    return false;
  }
  QFile::setPermissions(path, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
  return true;
}

} // namespace

QList<ApiAgent::Preset> ApiAgent::presets() {
  QList<Preset> found;
  const QJsonObject answer = chatRequest({{QStringLiteral("op"), QStringLiteral("api_presets")}});
  for (const QJsonValue& value : answer.value(QStringLiteral("agents")).toArray()) {
    const QJsonObject agent = value.toObject();
    Preset preset;
    preset.id = agent.value(QStringLiteral("id")).toString();
    preset.name = agent.value(QStringLiteral("name")).toString();
    preset.url = agent.value(QStringLiteral("url")).toString();
    preset.key = agent.value(QStringLiteral("key")).toString();
    preset.page = agent.value(QStringLiteral("page")).toString();
    if (!preset.id.isEmpty()) {
      found.append(preset);
    }
  }
  return found;
}

ApiAgent::ApiAgent(const Preset& preset, QObject* parent) : ChatAgent(parent), m_preset(preset) {
  m_network = new QNetworkAccessManager(this);
  // A key saved or removed in Preferences is read again.
  connect(&ChatNotifier::instance(), &ChatNotifier::apiSettingsChanged, this, [this] {
    m_keyKnown = false;
    m_key.clear();
  });
  const QJsonObject offered = chatRequest(
      {{QStringLiteral("op"), QStringLiteral("api_default_models")}, {QStringLiteral("agent"), m_preset.id}});
  QList<Model> models;
  for (const QJsonValue& value : offered.value(QStringLiteral("models")).toArray()) {
    const QJsonObject model = value.toObject();
    models.append(Model{model.value(QStringLiteral("id")).toString(), model.value(QStringLiteral("name")).toString()});
  }
  m_models = models;
}

ApiAgent::~ApiAgent() {
  ++m_generation;
  if (m_probe) {
    m_probe->disconnect(this);
    m_probe->abort();
  }
  shutdown();
}

QString ApiAgent::endpoint(QString* reason) const {
  const QString url = apiAgentUrl(m_preset.id, m_preset.url);
  if (url.isEmpty()) {
    if (reason != nullptr) {
      *reason = tr("%1 has no address: set it in Preferences › AI Assistants › Chat.").arg(name());
    }
    return QString();
  }
  const QJsonObject checked =
      chatRequest({{QStringLiteral("op"), QStringLiteral("api_endpoint")}, {QStringLiteral("url"), url}});
  if (checked.contains(QStringLiteral("error"))) {
    if (reason != nullptr) {
      *reason = tr("The address of %1 is not used: %2.").arg(name(), checked.value(QStringLiteral("error")).toString());
    }
    return QString();
  }
  return checked.value(QStringLiteral("url")).toString();
}

bool ApiAgent::installedNow() const { return apiAgentEnabled(m_preset.id); }

void ApiAgent::setReadiness(Readiness readiness, const QString& detail) {
  if (readiness == m_readiness && detail == m_detail) {
    return;
  }
  m_readiness = readiness;
  m_detail = detail;
  emit readinessChanged();
}

void ApiAgent::withKey(std::function<void(const QString& key)> done) {
  if (m_preset.key == QStringLiteral("none")) {
    done(QString());
    return;
  }
  if (m_keyKnown) {
    done(m_key);
    return;
  }
  if (sessionKeys().contains(m_preset.id)) {
    m_key = sessionKeys().value(m_preset.id);
    m_keyKnown = true;
    done(m_key);
    return;
  }
  if (keychainOff()) {
    m_key = m_preset.id == QStringLiteral("anthropic") ? qEnvironmentVariable("ANTHROPIC_API_KEY").trimmed() : QString();
    m_keyKnown = true;
    done(m_key);
    return;
  }
  auto* job = new QKeychain::ReadPasswordJob(kKeychainService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(m_preset.id);
  const int generation = m_generation;
  QObject::connect(job, &QKeychain::Job::finished, this, [this, generation, done](QKeychain::Job* finished) {
    auto* reading = static_cast<QKeychain::ReadPasswordJob*>(finished);
    QString key;
    if (reading->error() == QKeychain::NoError) {
      key = reading->textData().trimmed();
      qInfo().noquote() << QStringLiteral("Keychain: read %1's API key: found").arg(m_preset.id);
    } else {
      qInfo().noquote() << QStringLiteral("Keychain: read %1's API key: %2")
                               .arg(m_preset.id, reading->error() == QKeychain::EntryNotFound
                                                     ? QStringLiteral("not found")
                                                     : QStringLiteral("unavailable (%1)").arg(reading->errorString()));
    }
    if (key.isEmpty() && m_preset.id == QStringLiteral("anthropic")) {
      // The Anthropic API's key of MitCad's environment.
      key = qEnvironmentVariable("ANTHROPIC_API_KEY").trimmed();
      if (!key.isEmpty()) {
        qInfo().noquote() << QStringLiteral("Chat: %1 uses ANTHROPIC_API_KEY").arg(name());
      }
    }
    if (generation != m_generation && m_keyKnown) {
      return; // a key given meanwhile wins
    }
    m_key = key;
    m_keyKnown = true;
    done(m_key);
  });
  job->start();
}

void ApiAgent::check() {
  const int generation = ++m_generation;
  QString reason;
  const QString base = endpoint(&reason);
  if (base.isEmpty()) {
    setReadiness(Readiness::Missing, reason);
    return;
  }
  // Looked for again while missing: the state stays until it answers.
  if (m_readiness != Readiness::Missing) {
    setReadiness(Readiness::Checking);
  }
  withKey([this, generation, base](const QString& key) {
    if (generation != m_generation) {
      return;
    }
    if (m_preset.key == QStringLiteral("required") && key.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Chat: %1 has no API key").arg(name());
      setReadiness(Readiness::SignedOut);
      return;
    }
    listModels(base, key, generation);
  });
}

void ApiAgent::listModels(const QString& base, const QString& key, int generation) {
  const QJsonObject asked = chatRequest({{QStringLiteral("op"), QStringLiteral("api_models_request")},
                                         {QStringLiteral("agent"), m_preset.id},
                                         {QStringLiteral("url"), base}});
  if (asked.contains(QStringLiteral("error"))) {
    setReadiness(Readiness::Missing, asked.value(QStringLiteral("error")).toString());
    return;
  }
  QNetworkRequest request{QUrl(asked.value(QStringLiteral("url")).toString())};
  const QJsonObject headers = asked.value(QStringLiteral("headers")).toObject();
  for (auto header = headers.begin(); header != headers.end(); ++header) {
    request.setRawHeader(header.key().toUtf8(), header.value().toString().toUtf8());
  }
  const QString auth = asked.value(QStringLiteral("auth")).toString();
  if (!key.isEmpty() && auth == QStringLiteral("x-api-key")) {
    request.setRawHeader("x-api-key", key.toUtf8());
  } else if (!key.isEmpty() && auth == QStringLiteral("bearer")) {
    request.setRawHeader("Authorization", QByteArrayLiteral("Bearer ") + key.toUtf8());
  }
  request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
  request.setTransferTimeout(15000);
  if (m_probe) {
    m_probe->disconnect(this);
    m_probe->abort();
    m_probe->deleteLater();
  }
  QNetworkReply* reply = m_network->get(request);
  m_probe = reply;
  connect(reply, &QNetworkReply::finished, this, [this, reply, generation, base] {
    reply->deleteLater();
    if (reply != m_probe || generation != m_generation) {
      return;
    }
    m_probe = nullptr;
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QByteArray body = reply->read(kMaxModelsBody);
    const QString network = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();
    const QJsonObject listed = chatRequest({{QStringLiteral("op"), QStringLiteral("api_models")},
                                            {QStringLiteral("agent"), m_preset.id},
                                            {QStringLiteral("status"), status},
                                            {QStringLiteral("body"), QString::fromUtf8(body)},
                                            {QStringLiteral("error"), network}});
    const QJsonValue account = listed.value(QStringLiteral("account"));
    const QString code = account.toObject().value(QStringLiteral("code")).toString();
    if (listed.value(QStringLiteral("ok")).toBool()) {
      setModels(listed.value(QStringLiteral("models")).toArray());
      qInfo().noquote() << QStringLiteral("Chat: %1 answers at %2").arg(name(), base);
      setReadiness(Readiness::Ready);
      return;
    }
    if (status == 0) {
      // No answer: a local model's server not running, no network.
      const QString why = listed.value(QStringLiteral("text")).toString();
      qInfo().noquote() << QStringLiteral("Chat: no answer from %1 at %2: %3").arg(name(), base, why);
      const bool local = chatRequest({{QStringLiteral("op"), QStringLiteral("api_endpoint")}, {QStringLiteral("url"), base}})
                             .value(QStringLiteral("local"))
                             .toBool();
      setReadiness(Readiness::Missing, local && !m_preset.page.isEmpty()
                                           ? tr("No answer from %1 at %2. Start it, or get it.").arg(name(), base)
                                           : tr("No answer from %1 at %2: %3").arg(name(), base, why));
      return;
    }
    if ((code == QStringLiteral("signed-out") || code == QStringLiteral("invalid-key")) &&
        m_preset.key != QStringLiteral("none")) {
      qInfo().noquote() << QStringLiteral("Chat: %1 refused the API key").arg(name());
      setReadiness(Readiness::SignedOut, tr("%1 refused the API key. Enter a valid one.").arg(name()));
      return;
    }
    // It answers, but lists no models here (or the account has a limit):
    // usable; a message tells more.
    qInfo().noquote() << QStringLiteral("Chat: %1 answers at %2 without its models (%3)")
                             .arg(name(), base, listed.value(QStringLiteral("text")).toString());
    setModels(listed.value(QStringLiteral("models")).toArray());
    setReadiness(Readiness::Ready);
  });
}

void ApiAgent::setModels(const QJsonArray& listed) {
  QList<Model> found;
  for (const QJsonValue& value : listed) {
    const QJsonObject model = value.toObject();
    const QString identifier = model.value(QStringLiteral("id")).toString();
    if (!identifier.isEmpty()) {
      found.append(Model{identifier, model.value(QStringLiteral("name")).toString(identifier)});
    }
  }
  if (found.isEmpty() && !m_models.isEmpty() && m_preset.id == QStringLiteral("anthropic")) {
    return; // the current models stay offered
  }
  bool same = found.size() == m_models.size();
  for (qsizetype index = 0; same && index < found.size(); ++index) {
    same = found.at(index).id == m_models.at(index).id && found.at(index).name == m_models.at(index).name;
  }
  if (same) {
    return;
  }
  m_models = found;
  qInfo().noquote() << QStringLiteral("Chat: %1 offers %2 model(s)").arg(name()).arg(found.size());
  emit modelsChanged();
}

void ApiAgent::setKey(const QString& key) {
  const QString given = key.trimmed();
  if (given.isEmpty()) {
    return;
  }
  m_key = given;
  m_keyKnown = true;
  ++m_generation;
  qInfo().noquote() << QStringLiteral("Chat: an API key was given for %1").arg(name());
  saveApiKey(m_preset.id, given, this, [this](bool keychain, const QString& error) {
    Q_UNUSED(error);
    qInfo().noquote() << (keychain ? QStringLiteral("Chat: %1's API key is kept in the keychain").arg(name())
                                   : QStringLiteral("Chat: %1's API key is kept for this run").arg(name()));
  });
  check();
}

QString ApiAgent::installAction() const {
  if (m_preset.page.isEmpty()) {
    return tr("Chat Preferences…");
  }
  if (m_preset.key == QStringLiteral("required")) {
    return tr("Get an API Key");
  }
  return tr("Get %1").arg(name());
}

QString ApiAgent::signedOutText() const {
  return m_detail.isEmpty() ? tr("%1 needs an API key.").arg(name()) : m_detail;
}

QString ApiAgent::privacyNotice() const {
  const QString base = endpoint();
  const bool local = !base.isEmpty() && chatRequest({{QStringLiteral("op"), QStringLiteral("api_endpoint")},
                                                     {QStringLiteral("url"), base}})
                                            .value(QStringLiteral("local"))
                                            .toBool();
  if (local) {
    return tr("What you write here, the selection, the open comments and pictures of the view go to %1 at %2, on this "
              "computer. The model changes this design through MitCad's tools.")
        .arg(name(), base);
  }
  return tr("What you write here, the selection, the open comments and pictures of the view go to %1 at %2, whose "
            "terms apply. The model changes this design through MitCad's tools.")
      .arg(name(), base.isEmpty() ? m_preset.url : base);
}

QString ApiAgent::accountHelp() const {
  return tr("Enter a valid API key for %1 in Preferences › AI Assistants › Chat (Key…), check the account's billing, "
            "or wait for a rate limit to reset. Then choose Check again. The check sends one short request without your "
            "design or conversation; account usage may apply.")
      .arg(name());
}

bool ApiAgent::running() const { return m_stream.has_value(); }

bool ApiAgent::control(const QJsonObject& request, QString* error) {
  if (!m_stream) {
    if (error != nullptr) {
      *error = tr("%1 is not connected.").arg(name());
    }
    return false;
  }
  const QByteArray text = QJsonDocument(request).toJson(QJsonDocument::Compact);
  const std::string answer((*m_stream)->control(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
  const QJsonArray events = eventsOf(answer);
  if (events.size() == 1 && events.at(0).toObject().value(QStringLiteral("kind")).toString() == QStringLiteral("refused")) {
    if (error != nullptr) {
      *error = events.at(0).toObject().value(QStringLiteral("text")).toString();
    }
    return false;
  }
  handle(events);
  return true;
}

bool ApiAgent::start(const Start& start, QString* error) {
  if (running()) {
    return true;
  }
  QString reason;
  const QString base = endpoint(&reason);
  if (base.isEmpty()) {
    if (error != nullptr) {
      *error = reason;
    }
    return false;
  }
  // MitCad's own listener: the record of this instance, its port and token.
  mcp::Endpoint listener;
  for (const mcp::Endpoint& record : mcp::discover()) {
    if (record.instance == start.instance) {
      listener = record;
    }
  }
  if (listener.port == 0 || listener.token.isEmpty()) {
    if (error != nullptr) {
      *error = tr("MitCad's listener for assistants was not found.");
    }
    return false;
  }
  QString model = start.model;
  if (model.isEmpty() && m_preset.id != QStringLiteral("anthropic") && !m_models.isEmpty()) {
    model = m_models.first().id; // a local server's first model
  }
  try {
    m_stream.emplace(new_chat_stream(m_preset.id.toStdString()));
  } catch (const std::exception& failure) {
    if (error != nullptr) {
      *error = QString::fromUtf8(failure.what());
    }
    return false;
  }
  ++m_conversation;
  m_input.clear();
  m_outbox.clear();
  m_authenticated = false;
  m_auth = mcp::encodeAuth(listener, start.chat);
  m_socket = new QTcpSocket(this);
  m_socket->setReadBufferSize(0);
  QTcpSocket* socket = m_socket;
  connect(socket, &QTcpSocket::connected, this, [this, socket] {
    if (socket == m_socket) {
      m_socket->write(m_auth);
    }
  });
  connect(socket, &QTcpSocket::readyRead, this, [this, socket] {
    if (socket == m_socket) {
      readMcp();
    }
  });
  connect(socket, &QTcpSocket::disconnected, this, [this, socket] {
    if (socket == m_socket) {
      endConversation(tr("MitCad's listener closed the chat's connection."));
    }
  });
  connect(socket, &QTcpSocket::errorOccurred, this, [this, socket](QAbstractSocket::SocketError) {
    if (socket == m_socket && !m_authenticated) {
      endConversation(tr("The chat could not connect to MitCad's listener: %1").arg(m_socket->errorString()));
    }
  });
  const int conversation = m_conversation;
  QTimer::singleShot(kConnectMs, this, [this, conversation] {
    if (conversation == m_conversation && m_stream && !m_authenticated) {
      endConversation(tr("MitCad's listener did not accept the chat's connection."));
    }
  });
  socket->connectToHost(QHostAddress(QHostAddress::LocalHost), listener.port);
  const QString transcript = start.resume ? transcriptOf(start.session) : QString();
  qInfo().noquote() << (start.resume ? QStringLiteral("Chat: %1 started (resuming session %2%3)")
                                           .arg(name(), start.session,
                                                transcript.isEmpty() ? QStringLiteral(", nothing kept") : QString())
                                     : QStringLiteral("Chat: %1 started (new session %2)").arg(name(), start.session));
  const bool started = control({{QStringLiteral("op"), QStringLiteral("start")},
                                {QStringLiteral("name"), name()},
                                {QStringLiteral("url"), base},
                                {QStringLiteral("model"), model},
                                {QStringLiteral("session"), start.session},
                                {QStringLiteral("resume"), start.resume},
                                {QStringLiteral("transcript"), transcript}},
                               error);
  if (!started) {
    ++m_conversation;
    m_stream.reset();
    QTcpSocket* failed = m_socket;
    m_socket = nullptr;
    failed->disconnect(this);
    failed->abort();
    failed->deleteLater();
    return false;
  }
  return true;
}

bool ApiAgent::send(const Message& message, QString* error) {
  QJsonArray pictures;
  for (const Picture& picture : message.pictures) {
    pictures.append(QJsonObject{{QStringLiteral("media_type"), picture.mediaType},
                                {QStringLiteral("data"), QString::fromLatin1(picture.base64)}});
  }
  return control({{QStringLiteral("op"), QStringLiteral("message")},
                  {QStringLiteral("text"), message.text},
                  {QStringLiteral("context"), message.context},
                  {QStringLiteral("images"), pictures}},
                 error);
}

void ApiAgent::handle(const QJsonArray& events) {
  const int conversation = m_conversation;
  QString ending;
  for (const QJsonValue& value : events) {
    if (conversation != m_conversation) {
      return; // ended meanwhile (a handler shut the conversation down)
    }
    const QJsonObject item = value.toObject();
    const QString kind = item.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("mcp")) {
      writeMcp(item.value(QStringLiteral("line")).toString().toUtf8());
    } else if (kind == QStringLiteral("http")) {
      sendHttp(item);
    } else if (kind == QStringLiteral("http_abort")) {
      abortHttp(static_cast<quint64>(item.value(QStringLiteral("id")).toInteger()));
    } else if (kind == QStringLiteral("transcript")) {
      keepTranscript(item.value(QStringLiteral("session")).toString(), item.value(QStringLiteral("content")).toString());
    } else if (kind == QStringLiteral("end")) {
      ending = item.value(QStringLiteral("text")).toString();
    } else {
      emit panelEvent(item);
    }
  }
  if (!ending.isEmpty() && conversation == m_conversation) {
    // After the events before it (the failed init): queued, so that no
    // caller of the stream sees it go.
    QTimer::singleShot(0, this, [this, conversation, ending] {
      if (conversation == m_conversation) {
        endConversation(ending);
      }
    });
  }
}

void ApiAgent::writeMcp(const QByteArray& line) {
  if (!m_authenticated) {
    m_outbox.append(line);
    return;
  }
  if (m_socket != nullptr && m_socket->write(line) != line.size()) {
    qWarning().noquote() << QStringLiteral("Chat: a line could not be given to MitCad's listener");
  }
}

void ApiAgent::readMcp() {
  const int conversation = m_conversation;
  m_input += m_socket->readAll();
  qsizetype from = 0;
  for (qsizetype newline = m_input.indexOf('\n'); newline >= 0; newline = m_input.indexOf('\n', from)) {
    const QByteArray line = m_input.mid(from, newline - from);
    from = newline + 1;
    if (!m_authenticated) {
      const QJsonObject answer = QJsonDocument::fromJson(line).object();
      if (answer.value(QStringLiteral("authenticated")) != QJsonValue(true)) {
        m_input.clear();
        endConversation(tr("MitCad's listener refused the chat's connection (%1).")
                            .arg(answer.value(QStringLiteral("error")).toString(tr("not authenticated"))));
        return;
      }
      m_authenticated = true;
      const QList<QByteArray> waiting = std::exchange(m_outbox, {});
      for (const QByteArray& queued : waiting) {
        writeMcp(queued);
      }
      continue;
    }
    if (line.size() > kMaxLine) {
      qWarning().noquote() << QStringLiteral("Chat: a line of MitCad's listener of %1 bytes was skipped").arg(line.size());
      continue;
    }
    const QByteArray text = QString::fromUtf8(line).toUtf8();
    const std::string events((*m_stream)->feed_line(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
    handle(eventsOf(events));
    if (conversation != m_conversation || !m_stream) {
      return; // ended meanwhile
    }
  }
  m_input.remove(0, from);
  if (m_input.size() > kMaxLine) {
    qWarning().noquote() << QStringLiteral("Chat: a line of MitCad's listener over %1 bytes was skipped").arg(kMaxLine);
    m_input.clear();
  }
}

void ApiAgent::sendHttp(const QJsonObject& asked) {
  const quint64 id = static_cast<quint64>(asked.value(QStringLiteral("id")).toInteger());
  const QUrl url(asked.value(QStringLiteral("url")).toString());
  QNetworkRequest request(url);
  const QJsonObject headers = asked.value(QStringLiteral("headers")).toObject();
  for (auto header = headers.begin(); header != headers.end(); ++header) {
    request.setRawHeader(header.key().toUtf8(), header.value().toString().toUtf8());
  }
  // The key only in its header; never logged.
  const QString auth = asked.value(QStringLiteral("auth")).toString();
  if (!m_key.isEmpty() && auth == QStringLiteral("x-api-key")) {
    request.setRawHeader("x-api-key", m_key.toUtf8());
  } else if (!m_key.isEmpty() && auth == QStringLiteral("bearer")) {
    request.setRawHeader("Authorization", QByteArrayLiteral("Bearer ") + m_key.toUtf8());
  }
  // A redirect would carry the key elsewhere: not followed.
  request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
  request.setTransferTimeout(apiRequestTimeout() * 1000);
  const QByteArray body = asked.value(QStringLiteral("body")).toString().toUtf8();
  QNetworkReply* reply = asked.value(QStringLiteral("method")).toString() == QStringLiteral("GET")
                             ? m_network->get(request)
                             : m_network->post(request, body);
  Request kept;
  kept.reply = reply;
  m_requests.insert(id, kept);
  qInfo().noquote() << QStringLiteral("Chat: %1 asked (%2 bytes)").arg(url.toString(QUrl::RemoveQuery)).arg(body.size());
  connect(reply, &QNetworkReply::readyRead, this, [this, id] { readHttp(id, false); });
  connect(reply, &QNetworkReply::finished, this, [this, id] { readHttp(id, true); });
}

void ApiAgent::readHttp(quint64 id, bool finished) {
  const auto found = m_requests.find(id);
  if (found == m_requests.end() || !found->reply) {
    return;
  }
  QNetworkReply* reply = found->reply;
  const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
  found->pending += reply->readAll();
  QList<QByteArray> lines;
  qsizetype from = 0;
  for (qsizetype newline = found->pending.indexOf('\n'); newline >= 0;
       newline = found->pending.indexOf('\n', from)) {
    lines.append(found->pending.mid(from, newline - from));
    from = newline + 1;
  }
  found->pending.remove(0, from);
  if (found->pending.size() > kMaxLine) {
    found->pending.clear();
  }
  if (finished && !found->pending.isEmpty()) {
    lines.append(std::exchange(found->pending, {}));
  }
  const bool aborted = found->aborted;
  QString network;
  if (finished && reply->error() != QNetworkReply::NoError) {
    network = reply->error() == QNetworkReply::OperationCanceledError && !aborted
                  ? tr("no data for %1 s (the request timeout of Preferences › AI Assistants › Chat)")
                        .arg(apiRequestTimeout())
                  : reply->errorString();
  }
  if (finished) {
    m_requests.erase(found);
    reply->deleteLater();
  }
  if (aborted) {
    return;
  }
  const int conversation = m_conversation;
  for (const QByteArray& line : std::as_const(lines)) {
    if (conversation != m_conversation) {
      return;
    }
    control({{QStringLiteral("op"), QStringLiteral("http_line")},
             {QStringLiteral("id"), static_cast<qint64>(id)},
             {QStringLiteral("status"), status},
             {QStringLiteral("line"), QString::fromUtf8(line)}},
            nullptr);
  }
  if (finished && conversation == m_conversation) {
    if (!network.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Chat: %1's request ended: HTTP %2, %3").arg(name()).arg(status).arg(network);
    }
    control({{QStringLiteral("op"), QStringLiteral("http_end")},
             {QStringLiteral("id"), static_cast<qint64>(id)},
             {QStringLiteral("status"), status},
             {QStringLiteral("error"), network}},
            nullptr);
  }
}

void ApiAgent::abortHttp(quint64 id) {
  const auto found = m_requests.find(id);
  if (found == m_requests.end()) {
    return;
  }
  QPointer<QNetworkReply> reply = found->reply;
  m_requests.erase(found);
  if (reply) {
    reply->disconnect(this);
    reply->abort();
    reply->deleteLater();
  }
  qInfo().noquote() << QStringLiteral("Chat: %1's request stopped").arg(name());
}

void ApiAgent::keepTranscript(const QString& session, const QString& content) {
  if (!ChatSettings::load().history || !plainSession(session)) {
    return;
  }
  const QString folder = apiSessionDirectory();
  if (!privateFolder(folder)) {
    qWarning().noquote() << QStringLiteral("Chat: the conversation could not be kept");
    return;
  }
  QSaveFile file(QDir(folder).filePath(session + QStringLiteral(".json")));
  const QByteArray bytes = content.toUtf8();
  if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit()) {
    qWarning().noquote() << QStringLiteral("Chat: the conversation could not be kept");
    return;
  }
  QFile::setPermissions(file.fileName(), QFileDevice::ReadOwner | QFileDevice::WriteOwner);
}

QString ApiAgent::transcriptOf(const QString& session) const {
  if (!plainSession(session)) {
    return QString();
  }
  QFile file(QDir(apiSessionDirectory()).filePath(session + QStringLiteral(".json")));
  if (!file.open(QIODevice::ReadOnly)) {
    return QString();
  }
  return QString::fromUtf8(file.read(16 * 1024 * 1024));
}

void ApiAgent::forgetSession(const QString& session) {
  if (session.isEmpty()) {
    clearApiSessions();
  } else if (plainSession(session)) {
    QFile::remove(QDir(apiSessionDirectory()).filePath(session + QStringLiteral(".json")));
  }
}

void ApiAgent::stop() {
  if (!running()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Chat: stopping %1").arg(name());
  QString error;
  if (!control({{QStringLiteral("op"), QStringLiteral("interrupt")}}, &error)) {
    qInfo().noquote() << QStringLiteral("Chat: %1's message could not be stopped (%2)").arg(name(), error);
  }
}

void ApiAgent::endConversation(const QString& text) {
  if (!m_stream) {
    return;
  }
  ++m_conversation;
  m_stream.reset();
  for (auto request = m_requests.begin(); request != m_requests.end(); ++request) {
    if (request->reply) {
      request->reply->disconnect(this);
      request->reply->abort();
      request->reply->deleteLater();
    }
  }
  m_requests.clear();
  if (m_socket != nullptr) {
    QTcpSocket* socket = m_socket;
    m_socket = nullptr;
    socket->disconnect(this);
    socket->abort();
    socket->deleteLater();
  }
  m_authenticated = false;
  m_outbox.clear();
  m_input.clear();
  if (text.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Chat: %1's conversation closed").arg(name());
    return;
  }
  qInfo().noquote() << QStringLiteral("Chat: %1 ended: %2").arg(name(), text);
  emit exited(1, text, false);
}

void ApiAgent::shutdown() { endConversation(QString()); }

void saveApiKey(const QString& agent, const QString& key, QObject* context,
                std::function<void(bool keychain, const QString& error)> done) {
  sessionKeys().insert(agent, key);
  if (keychainOff()) {
    qInfo().noquote() << QStringLiteral("Keychain: off (test); %1's API key kept for this run").arg(agent);
    QTimer::singleShot(0, context, [done = std::move(done)] { done(false, QString()); });
    return;
  }
  auto* job = new QKeychain::WritePasswordJob(kKeychainService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(agent);
  job->setTextData(key);
  QObject::connect(job, &QKeychain::Job::finished, context, [agent, done = std::move(done)](QKeychain::Job* finished) {
    const bool kept = finished->error() == QKeychain::NoError;
    qInfo().noquote() << (kept ? QStringLiteral("Keychain: saved %1's API key").arg(agent)
                               : QStringLiteral("Keychain: save %1's API key failed (%2); kept for this run")
                                     .arg(agent, finished->errorString()));
    done(kept, QString());
  });
  job->start();
}

void removeApiKey(const QString& agent, QObject* context, std::function<void(const QString& error)> done) {
  sessionKeys().remove(agent);
  if (keychainOff()) {
    QTimer::singleShot(0, context, [done = std::move(done)] { done(QString()); });
    return;
  }
  auto* job = new QKeychain::DeletePasswordJob(kKeychainService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(agent);
  QObject::connect(job, &QKeychain::Job::finished, context, [agent, done = std::move(done)](QKeychain::Job* finished) {
    const QString error = finished->error() == QKeychain::NoError || finished->error() == QKeychain::EntryNotFound
                              ? QString()
                              : finished->errorString();
    qInfo().noquote() << (error.isEmpty() ? QStringLiteral("Keychain: removed %1's API key").arg(agent)
                                          : QStringLiteral("Keychain: remove %1's API key failed (%2)").arg(agent, error));
    done(error);
  });
  job->start();
}

bool clearApiSessions() {
  const QString folder = apiSessionDirectory();
  if (folder.isEmpty() || !QFileInfo::exists(folder)) {
    return true;
  }
  QDir directory(folder);
  bool cleared = true;
  const QStringList files = directory.entryList({QStringLiteral("*.json")}, QDir::Files);
  for (const QString& name : files) {
    cleared = directory.remove(name) && cleared;
  }
  return cleared;
}

} // namespace mitcad::agent
