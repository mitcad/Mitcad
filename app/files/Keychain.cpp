// SPDX-License-Identifier: MIT
#include "Keychain.hpp"

#include <utility>

#include <QJsonDocument>
#include <QJsonObject>
#include <QPointer>
#include <QtLogging>

#include <qtkeychain/keychain.h>

namespace mitcad::keychain {
namespace {

const QString kService = QStringLiteral("Mitcad live updates");

// The entry's text: the user name and the password together.
QString encode(const BrokerCredentials& credentials) {
  const QJsonObject entry{{QStringLiteral("format"), QStringLiteral("mitcad-broker-credentials")},
                          {QStringLiteral("user"), credentials.user},
                          {QStringLiteral("password"), credentials.password}};
  return QString::fromUtf8(QJsonDocument(entry).toJson(QJsonDocument::Compact));
}

std::optional<BrokerCredentials> decode(const QString& text) {
  const QJsonObject entry = QJsonDocument::fromJson(text.toUtf8()).object();
  if (entry.value(QStringLiteral("format")).toString() != QLatin1String("mitcad-broker-credentials") ||
      entry.value(QStringLiteral("user")).toString().isEmpty()) {
    return std::nullopt;
  }
  return BrokerCredentials{entry.value(QStringLiteral("user")).toString(),
                           entry.value(QStringLiteral("password")).toString()};
}

} // namespace

void read(const QString& key, QObject* context,
          std::function<void(Read result, const BrokerCredentials& credentials, const QString& error)> done) {
  auto* job = new QKeychain::ReadPasswordJob(kService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(key);
  QObject::connect(job, &QKeychain::Job::finished, context,
                   [key, done = std::move(done)](QKeychain::Job* finished) {
                     auto* reading = static_cast<QKeychain::ReadPasswordJob*>(finished);
                     if (reading->error() == QKeychain::NoError) {
                       const std::optional<BrokerCredentials> credentials = decode(reading->textData());
                       if (credentials) {
                         qInfo().noquote() << QStringLiteral("Keychain: read %1: found, user %2")
                                                  .arg(key, credentials->user);
                         done(Read::Found, *credentials, QString());
                         return;
                       }
                       qInfo().noquote() << QStringLiteral("Keychain: read %1: not Mitcad's entry").arg(key);
                       done(Read::NotFound, {}, QString());
                       return;
                     }
                     if (reading->error() == QKeychain::EntryNotFound) {
                       qInfo().noquote() << QStringLiteral("Keychain: read %1: not found").arg(key);
                       done(Read::NotFound, {}, QString());
                       return;
                     }
                     qInfo().noquote() << QStringLiteral("Keychain: read %1: unavailable (%2)")
                                              .arg(key, reading->errorString());
                     done(Read::Unavailable, {}, reading->errorString());
                   });
  job->start();
}

void write(const QString& key, const BrokerCredentials& credentials, QObject* context,
           std::function<void(const QString& error)> done) {
  auto* job = new QKeychain::WritePasswordJob(kService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(key);
  job->setTextData(encode(credentials));
  QObject::connect(job, &QKeychain::Job::finished, context,
                   [key, done = std::move(done)](QKeychain::Job* finished) {
                     const QString error =
                         finished->error() == QKeychain::NoError ? QString() : finished->errorString();
                     qInfo().noquote() << (error.isEmpty()
                                               ? QStringLiteral("Keychain: saved %1").arg(key)
                                               : QStringLiteral("Keychain: save %1 failed (%2)").arg(key, error));
                     done(error);
                   });
  job->start();
}

void remove(const QString& key, QObject* context, std::function<void(const QString& error)> done) {
  auto* job = new QKeychain::DeletePasswordJob(kService);
  job->setAutoDelete(true);
  job->setInsecureFallback(false);
  job->setKey(key);
  QObject::connect(job, &QKeychain::Job::finished, context,
                   [key, done = std::move(done)](QKeychain::Job* finished) {
                     // Nothing there is as good as removed.
                     const QString error = finished->error() == QKeychain::NoError ||
                                                   finished->error() == QKeychain::EntryNotFound
                                               ? QString()
                                               : finished->errorString();
                     qInfo().noquote() << (error.isEmpty()
                                               ? QStringLiteral("Keychain: removed %1").arg(key)
                                               : QStringLiteral("Keychain: remove %1 failed (%2)").arg(key, error));
                     done(error);
                   });
  job->start();
}

} // namespace mitcad::keychain
