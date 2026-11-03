// SPDX-License-Identifier: MIT
#pragma once

// API endpoints and local models in the chat panel (mitcad#947, phase 2):
// the Anthropic Messages API, Ollama, LM Studio and a custom
// OpenAI-compatible endpoint, each an agent behind the chat's one interface
// (ChatAgent). MitCad runs the tool loop itself, in process: there is no
// agent program and no bridge process.
//
// The protocols are the chat logic's (core/mcp/src/chat/api.rs): this class
// opens the chat's own connection to MitCad's live listener (its token and
// the chat's marker, as the bridge does, so the listener's approvals for
// the chat apply unchanged), sends the HTTP requests the stream asks for
// with Qt Network and gives the stream both sides' lines; the stream
// answers with lines to write, requests to send and panel events.
//
// Setup: the endpoint's address (Preferences › AI Assistants › Chat; the
// preset otherwise) must be https, or http to this computer only; the API
// key, where the service needs one, is kept in the system's keychain
// (QtKeychain, service "MitCad AI chat", one entry per agent), or for the
// session when there is no keychain; for the Anthropic API,
// ANTHROPIC_API_KEY of MitCad's environment when none is kept. The key
// goes only into the request's header, never into a log, a file of
// MitCad's or the chat logic. check() lists the service's models (GET
// /models): an answer makes the agent Ready, a refused key SignedOut, no
// answer Missing (a local model's server not running: the panel looks
// again while it shows, and goes on once it answers).
//
// Each request waits at most the requests' timeout (Preferences, seconds)
// for the service's next data. The conversation is kept for resuming in
// the chat folder's api-sessions/<session>.json (without pictures), when
// the history is kept.

#include <functional>
#include <optional>

#include <QByteArray>
#include <QHash>
#include <QJsonArray>
#include <QList>
#include <QPointer>
#include <QString>

#include "ChatAgent.hpp"
#include "mitcad_bridge/lib.h"

class QNetworkAccessManager;
class QNetworkReply;
class QTcpSocket;

namespace mitcad::agent {

class ApiAgent : public ChatAgent {
  Q_OBJECT

public:
  // An agent of the chat logic's api_presets: its id, name, preset address,
  // key ("required", "optional", "none") and setup page.
  struct Preset {
    QString id;
    QString name;
    QString url;
    QString key;
    QString page;
  };
  // Every API agent, in the switcher's order.
  static QList<Preset> presets();

  explicit ApiAgent(const Preset& preset, QObject* parent = nullptr);
  ~ApiAgent() override;

  QString id() const override { return m_preset.id; }
  QString name() const override { return m_preset.name; }
  QString installPage() const override { return m_preset.page; }
  QString accountName() const override { return m_preset.name; }
  QList<Model> models() const override { return m_models; }

  void check() override;
  // Offered in the panel (Preferences): its service is looked for then.
  bool installedNow() const override;
  Readiness readiness() const override { return m_readiness; }
  QString readinessDetail() const override { return m_detail; }
  void signIn() override {}

  bool running() const override;
  bool start(const Start& start, QString* error) override;
  bool send(const Message& message, QString* error) override;
  void stop() override;
  void shutdown() override;

  QString installAction() const override;
  QString signedOutText() const override;
  QString signInAction() const override { return tr("Save Key"); }
  bool asksForKey() const override { return m_preset.key != QStringLiteral("none"); }
  void setKey(const QString& key) override;
  QString privacyNotice() const override;
  QString accountHelp() const override;
  bool needsBridge() const override { return false; }
  void forgetSession(const QString& session) override;

  // The address in use: the Preferences' or the preset; empty with the
  // reason when it is not allowed.
  QString endpoint(QString* reason = nullptr) const;

private:
  struct Request {
    QPointer<QNetworkReply> reply;
    QByteArray pending;
    bool aborted = false;
  };

  void setReadiness(Readiness readiness, const QString& detail = QString());
  // The key (empty: none), from this run, the keychain or the environment.
  void withKey(std::function<void(const QString& key)> done);
  void listModels(const QString& base, const QString& key, int generation);
  void setModels(const QJsonArray& listed);
  bool control(const QJsonObject& request, QString* error);
  void handle(const QJsonArray& events);
  void writeMcp(const QByteArray& line);
  void readMcp();
  void sendHttp(const QJsonObject& request);
  void readHttp(quint64 id, bool finished);
  void abortHttp(quint64 id);
  void keepTranscript(const QString& session, const QString& content);
  QString transcriptOf(const QString& session) const;
  // The conversation ends: `text` says why (an empty text: shutdown).
  void endConversation(const QString& text);

  Preset m_preset;
  Readiness m_readiness = Readiness::Checking;
  QString m_detail;
  int m_generation = 0;
  QList<Model> m_models;
  QString m_key;
  bool m_keyKnown = false;

  QNetworkAccessManager* m_network = nullptr;
  QTcpSocket* m_socket = nullptr;
  QByteArray m_input;
  bool m_authenticated = false;
  QList<QByteArray> m_outbox;
  QByteArray m_auth;
  std::optional<rust::Box<ChatStream>> m_stream;
  int m_conversation = 0; // a new one on each start and end
  QHash<quint64, Request> m_requests;
  QPointer<QNetworkReply> m_probe;
};

// The API keys in the system's keychain (service "MitCad AI chat", one
// entry per agent id); without a keychain, kept for this run only. `done`
// gets an empty error when it was kept (in the keychain or for the run) and
// whether the keychain kept it.
void saveApiKey(const QString& agent, const QString& key, QObject* context,
                std::function<void(bool keychain, const QString& error)> done);
// Removed from the keychain and this run.
void removeApiKey(const QString& agent, QObject* context, std::function<void(const QString& error)> done);
// Every kept conversation of the API agents (Clear all history).
bool clearApiSessions();

} // namespace mitcad::agent
